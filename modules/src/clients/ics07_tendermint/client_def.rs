use core::convert::TryInto;

use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;
use prost::Message;
use tendermint_light_client_verifier::types::{TrustedBlockState, UntrustedBlockState};
use tendermint_light_client_verifier::{ProdVerifier, Verdict, Verifier};
use tendermint_proto::Protobuf;

use crate::clients::ics07_tendermint::client_state::ClientState as TmClientState;
use crate::clients::ics07_tendermint::consensus_state::ConsensusState as TmConsensusState;
use crate::clients::ics07_tendermint::error::Error;
use crate::clients::ics07_tendermint::header::Header as TmHeader;
use crate::core::ics02_client::client_consensus::ConsensusState;
use crate::core::ics02_client::client_def::{ClientDef, UpdatedState};
use crate::core::ics02_client::client_state::ClientState;
use crate::core::ics02_client::client_type::ClientType;
use crate::core::ics02_client::context::LightClientReader;
use crate::core::ics02_client::error::Error as Ics02Error;
use crate::core::ics02_client::header::Header;
use crate::core::ics03_connection::connection::ConnectionEnd;
use crate::core::ics04_channel::channel::ChannelEnd;
use crate::core::ics04_channel::commitment::{AcknowledgementCommitment, PacketCommitment};
use crate::core::ics04_channel::context::ChannelMetaReader;
use crate::core::ics04_channel::packet::Sequence;
use crate::core::ics23_commitment::commitment::{
    CommitmentPrefix, CommitmentProofBytes, CommitmentRoot,
};
use crate::core::ics23_commitment::merkle::{apply_prefix, MerkleProof};
use crate::core::ics24_host::identifier::ConnectionId;
use crate::core::ics24_host::identifier::{ChannelId, ClientId, PortId};
use crate::core::ics24_host::path::{
    AcksPath, ChannelEndsPath, ClientConsensusStatePath, ClientStatePath, CommitmentsPath,
    ConnectionsPath, ReceiptsPath, SeqRecvsPath,
};
use crate::core::ics24_host::Path;
use crate::prelude::*;
use crate::Height;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TendermintClient {
    verifier: ProdVerifier,
}

impl ClientDef for TendermintClient {
    fn check_header_and_update_state(
        &self,
        ctx: &dyn LightClientReader,
        client_id: ClientId,
        client_state: Box<dyn ClientState>,
        header: &dyn Header,
    ) -> Result<UpdatedState, Ics02Error> {
        if header.height().revision_number != client_state.chain_id().version() {
            return Err(Ics02Error::client_specific(
                Error::mismatched_revisions(
                    client_state.chain_id().version(),
                    header.height().revision_number,
                )
                .to_string(),
            ));
        }

        let header = downcast_header(header)?;

        // Check if a consensus state is already installed; if so it should
        // match the untrusted header.
        let header_consensus_state = TmConsensusState::from(header.clone());
        let existing_consensus_state =
            match ctx.maybe_consensus_state(&client_id, header.height())? {
                Some(cs) => {
                    let tm_cs = downcast_consensus_state(cs.as_ref()).map(Clone::clone)?;
                    // If this consensus state matches, skip verification
                    // (optimization)
                    if tm_cs == header_consensus_state {
                        // Header is already installed and matches the incoming
                        // header (already verified)
                        return Ok((client_state, cs).into());
                    }
                    Some(tm_cs)
                }
                None => None,
            };

        let trusted_state = {
            let trusted_cs = ctx.consensus_state(&client_id, header.trusted_height)?;
            let trusted_tm_cs = downcast_consensus_state(trusted_cs.as_ref())?;

            TrustedBlockState {
                header_time: trusted_tm_cs.timestamp,
                height: header
                    .trusted_height
                    .revision_height
                    .try_into()
                    .map_err(|_| {
                        Ics02Error::client_specific(
                            Error::invalid_header_height(header.trusted_height).to_string(),
                        )
                    })?,
                next_validators: &header.trusted_validator_set,
                next_validators_hash: trusted_tm_cs.next_validators_hash,
            }
        };

        let untrusted_state = UntrustedBlockState {
            signed_header: &header.signed_header,
            validators: &header.validator_set,
            // NB: This will skip the
            // VerificationPredicates::next_validators_match check for the
            // untrusted state.
            next_validators: None,
        };

        let client_state = downcast_client_state(client_state.as_ref())?;
        let options = client_state.as_light_client_options()?;

        let verdict = self.verifier.verify(
            untrusted_state,
            trusted_state,
            &options,
            ctx.host_timestamp().into_tm_time().unwrap(),
        );

        match verdict {
            Verdict::Success => {}
            Verdict::NotEnoughTrust(voting_power_tally) => {
                return Err(Error::not_enough_trusted_vals_signed(format!(
                    "voting power tally: {}",
                    voting_power_tally
                ))
                .into())
            }
            Verdict::Invalid(detail) => return Err(Error::verification_error(detail).into()),
        }

        // If the header has verified, but its corresponding consensus state
        // differs from the existing consensus state for that height, freeze the
        // client and return the installed consensus state.
        if let Some(cs) = existing_consensus_state {
            if cs != header_consensus_state {
                return Ok((
                    client_state
                        .clone()
                        .with_frozen_height(header.height())?
                        .boxed(),
                    cs.boxed(),
                )
                    .into());
            }
        }

        // Monotonicity checks for timestamps for in-the-middle updates
        // (cs-new, cs-next, cs-latest)
        if header.height() < client_state.latest_height() {
            let maybe_next_cs = ctx.next_consensus_state(&client_id, header.height())?;

            if let Some(next_cs) = maybe_next_cs {
                // New (untrusted) header timestamp cannot occur after next
                // consensus state's height
                if header.signed_header.header().time > next_cs.timestamp().into_tm_time().unwrap()
                {
                    return Err(Ics02Error::client_specific(
                        Error::header_timestamp_too_high(
                            header.signed_header.header().time.to_string(),
                            next_cs.timestamp().to_string(),
                        )
                        .to_string(),
                    ));
                }
            }
        }

        // (cs-trusted, cs-prev, cs-new)
        if header.trusted_height < header.height() {
            let maybe_prev_cs = ctx.prev_consensus_state(&client_id, header.height())?;

            if let Some(prev_cs) = maybe_prev_cs {
                // New (untrusted) header timestamp cannot occur before the
                // previous consensus state's height
                if header.signed_header.header().time < prev_cs.timestamp().into_tm_time().unwrap()
                {
                    return Err(Ics02Error::client_specific(
                        Error::header_timestamp_too_low(
                            header.signed_header.header().time.to_string(),
                            prev_cs.timestamp().to_string(),
                        )
                        .to_string(),
                    ));
                }
            }
        }

        Ok((
            client_state.clone().with_header(header.clone()).boxed(),
            TmConsensusState::from(header.clone()).boxed(),
        )
            .into())
    }

    fn verify_client_consensus_state(
        &self,
        client_state: &dyn ClientState,
        height: Height,
        prefix: &CommitmentPrefix,
        proof: &CommitmentProofBytes,
        root: &CommitmentRoot,
        client_id: &ClientId,
        consensus_height: Height,
        expected_consensus_state: &dyn ConsensusState,
    ) -> Result<(), Ics02Error> {
        let client_state = downcast_client_state(client_state)?;
        client_state.verify_height(height)?;

        let path = ClientConsensusStatePath {
            client_id: client_id.clone(),
            epoch: consensus_height.revision_number,
            height: consensus_height.revision_height,
        };
        let value = expected_consensus_state.encode_vec();
        verify_membership(client_state, prefix, proof, root, path, value)
    }

    fn verify_connection_state(
        &self,
        client_state: &dyn ClientState,
        height: Height,
        prefix: &CommitmentPrefix,
        proof: &CommitmentProofBytes,
        root: &CommitmentRoot,
        connection_id: &ConnectionId,
        expected_connection_end: &ConnectionEnd,
    ) -> Result<(), Ics02Error> {
        let client_state = downcast_client_state(client_state)?;
        client_state.verify_height(height)?;

        let path = ConnectionsPath(connection_id.clone());
        let value = expected_connection_end
            .encode_vec()
            .map_err(Ics02Error::invalid_connection_end)?;
        verify_membership(client_state, prefix, proof, root, path, value)
    }

    fn verify_channel_state(
        &self,
        client_state: &dyn ClientState,
        height: Height,
        prefix: &CommitmentPrefix,
        proof: &CommitmentProofBytes,
        root: &CommitmentRoot,
        port_id: &PortId,
        channel_id: &ChannelId,
        expected_channel_end: &ChannelEnd,
    ) -> Result<(), Ics02Error> {
        let client_state = downcast_client_state(client_state)?;
        client_state.verify_height(height)?;

        let path = ChannelEndsPath(port_id.clone(), *channel_id);
        let value = expected_channel_end
            .encode_vec()
            .map_err(Ics02Error::invalid_channel_end)?;
        verify_membership(client_state, prefix, proof, root, path, value)
    }

    fn verify_client_full_state(
        &self,
        client_state: &dyn ClientState,
        height: Height,
        prefix: &CommitmentPrefix,
        proof: &CommitmentProofBytes,
        root: &CommitmentRoot,
        client_id: &ClientId,
        expected_client_state: &dyn ClientState,
    ) -> Result<(), Ics02Error> {
        let client_state = downcast_client_state(client_state)?;
        client_state.verify_height(height)?;

        let path = ClientStatePath(client_id.clone());
        let value = expected_client_state.encode_vec();
        verify_membership(client_state, prefix, proof, root, path, value)
    }

    fn verify_packet_data(
        &self,
        ctx: &dyn ChannelMetaReader,
        client_state: &dyn ClientState,
        height: Height,
        connection_end: &ConnectionEnd,
        proof: &CommitmentProofBytes,
        root: &CommitmentRoot,
        port_id: &PortId,
        channel_id: &ChannelId,
        sequence: Sequence,
        commitment: PacketCommitment,
    ) -> Result<(), Ics02Error> {
        let client_state = downcast_client_state(client_state)?;
        client_state.verify_height(height)?;
        verify_delay_passed(ctx, height, connection_end)?;

        let commitment_path = CommitmentsPath {
            port_id: port_id.clone(),
            channel_id: *channel_id,
            sequence,
        };

        verify_membership(
            client_state,
            connection_end.counterparty().prefix(),
            proof,
            root,
            commitment_path,
            commitment.into_vec(),
        )
    }

    fn verify_packet_acknowledgement(
        &self,
        ctx: &dyn ChannelMetaReader,
        client_state: &dyn ClientState,
        height: Height,
        connection_end: &ConnectionEnd,
        proof: &CommitmentProofBytes,
        root: &CommitmentRoot,
        port_id: &PortId,
        channel_id: &ChannelId,
        sequence: Sequence,
        ack_commitment: AcknowledgementCommitment,
    ) -> Result<(), Ics02Error> {
        let client_state = downcast_client_state(client_state)?;
        client_state.verify_height(height)?;
        verify_delay_passed(ctx, height, connection_end)?;

        let ack_path = AcksPath {
            port_id: port_id.clone(),
            channel_id: *channel_id,
            sequence,
        };
        verify_membership(
            client_state,
            connection_end.counterparty().prefix(),
            proof,
            root,
            ack_path,
            ack_commitment.into_vec(),
        )
    }

    fn verify_next_sequence_recv(
        &self,
        ctx: &dyn ChannelMetaReader,
        client_state: &dyn ClientState,
        height: Height,
        connection_end: &ConnectionEnd,
        proof: &CommitmentProofBytes,
        root: &CommitmentRoot,
        port_id: &PortId,
        channel_id: &ChannelId,
        sequence: Sequence,
    ) -> Result<(), Ics02Error> {
        let client_state = downcast_client_state(client_state)?;
        client_state.verify_height(height)?;
        verify_delay_passed(ctx, height, connection_end)?;

        let mut seq_bytes = Vec::new();
        u64::from(sequence)
            .encode(&mut seq_bytes)
            .expect("buffer size too small");

        let seq_path = SeqRecvsPath(port_id.clone(), *channel_id);
        verify_membership(
            client_state,
            connection_end.counterparty().prefix(),
            proof,
            root,
            seq_path,
            seq_bytes,
        )
    }

    fn verify_packet_receipt_absence(
        &self,
        ctx: &dyn ChannelMetaReader,
        client_state: &dyn ClientState,
        height: Height,
        connection_end: &ConnectionEnd,
        proof: &CommitmentProofBytes,
        root: &CommitmentRoot,
        port_id: &PortId,
        channel_id: &ChannelId,
        sequence: Sequence,
    ) -> Result<(), Ics02Error> {
        let client_state = downcast_client_state(client_state)?;
        client_state.verify_height(height)?;
        verify_delay_passed(ctx, height, connection_end)?;

        let receipt_path = ReceiptsPath {
            port_id: port_id.clone(),
            channel_id: *channel_id,
            sequence,
        };
        verify_non_membership(
            client_state,
            connection_end.counterparty().prefix(),
            proof,
            root,
            receipt_path,
        )
    }

    fn verify_upgrade_and_update_state(
        &self,
        _client_state: &dyn ClientState,
        _consensus_state: &dyn ConsensusState,
        _proof_upgrade_client: RawMerkleProof,
        _proof_upgrade_consensus_state: RawMerkleProof,
    ) -> Result<UpdatedState, Ics02Error> {
        todo!()
    }
}

fn verify_membership(
    client_state: &TmClientState,
    prefix: &CommitmentPrefix,
    proof: &CommitmentProofBytes,
    root: &CommitmentRoot,
    path: impl Into<Path>,
    value: Vec<u8>,
) -> Result<(), Ics02Error> {
    let merkle_path = apply_prefix(prefix, vec![path.into().to_string()]);
    let merkle_proof: MerkleProof = RawMerkleProof::try_from(proof.clone())
        .map_err(Ics02Error::invalid_commitment_proof)?
        .into();

    merkle_proof
        .verify_membership(
            &client_state.proof_specs,
            root.clone().into(),
            merkle_path,
            value,
            0,
        )
        .map_err(Ics02Error::ics23_verification)
}

fn verify_non_membership(
    client_state: &TmClientState,
    prefix: &CommitmentPrefix,
    proof: &CommitmentProofBytes,
    root: &CommitmentRoot,
    path: impl Into<Path>,
) -> Result<(), Ics02Error> {
    let merkle_path = apply_prefix(prefix, vec![path.into().to_string()]);
    let merkle_proof: MerkleProof = RawMerkleProof::try_from(proof.clone())
        .map_err(Ics02Error::invalid_commitment_proof)?
        .into();

    merkle_proof
        .verify_non_membership(&client_state.proof_specs, root.clone().into(), merkle_path)
        .map_err(Ics02Error::ics23_verification)
}

fn verify_delay_passed(
    ctx: &dyn ChannelMetaReader,
    height: Height,
    connection_end: &ConnectionEnd,
) -> Result<(), Ics02Error> {
    let current_timestamp = ctx.host_timestamp();
    let current_height = ctx.host_height();

    let client_id = connection_end.client_id();
    let processed_time = ctx
        .client_update_time(client_id, height)
        .map_err(|_| Error::processed_time_not_found(client_id.clone(), height))?;
    let processed_height = ctx
        .client_update_height(client_id, height)
        .map_err(|_| Error::processed_height_not_found(client_id.clone(), height))?;

    let delay_period_time = connection_end.delay_period();
    let delay_period_height = ctx.block_delay(delay_period_time);

    TmClientState::verify_delay_passed(
        current_timestamp,
        current_height,
        processed_time,
        processed_height,
        delay_period_time,
        delay_period_height,
    )
    .map_err(|e| e.into())
}

fn downcast_client_state(cs: &dyn ClientState) -> Result<&TmClientState, Ics02Error> {
    cs.as_any()
        .downcast_ref::<TmClientState>()
        .ok_or_else(|| Ics02Error::client_args_type_mismatch(ClientType::Tendermint))
}

fn downcast_consensus_state(cs: &dyn ConsensusState) -> Result<&TmConsensusState, Ics02Error> {
    cs.as_any()
        .downcast_ref::<TmConsensusState>()
        .ok_or_else(|| Ics02Error::client_args_type_mismatch(ClientType::Tendermint))
}

fn downcast_header(h: &dyn Header) -> Result<&TmHeader, Ics02Error> {
    h.as_any()
        .downcast_ref::<TmHeader>()
        .ok_or_else(|| Ics02Error::client_args_type_mismatch(ClientType::Tendermint))
}
