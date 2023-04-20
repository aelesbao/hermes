use core::time::Duration;

use flex_error::{define_error, ErrorMessageTracer};

use ibc_relayer_types::core::ics02_client::error::Error as ClientError;
use ibc_relayer_types::core::ics04_channel::channel::State;
use ibc_relayer_types::core::ics24_host::identifier::{
    ChainId, ChannelId, ClientId, PortChannelId, PortId,
};
use ibc_relayer_types::events::IbcEvent;
use ibc_relayer_types::proofs::ProofError;

use crate::error::Error as RelayerError;
use crate::foreign_client::{ForeignClientError, HasExpiredOrFrozenError};
use crate::supervisor::Error as SupervisorError;

define_error! {
    ChannelError {
        Relayer
            [ RelayerError ]
            |_| { "relayer error" },

        Supervisor
            [ SupervisorError ]
            |_| { "supervisor error" },

        Client
            [ ClientError ]
            |_| { "ICS02 client error" },

        InvalidChannel
            { reason: String }
            | e | {
                format_args!("invalid channel: {0}",
                    e.reason)
            },

        InvalidChannelUpgradeOrdering
            |_| { "attempted to upgrade a channel to a more strict ordring, which is not allowed" },

        InvalidChannelUpgradeState
            |_| { "attempted to upgrade a channel that is not in the OPEN state" },

        InvalidChannelUpgradeTimeout
            |_| { "attempted to upgrade a channel without supplying at least one of timeout height or timeout timestamp" },

        MissingLocalChannelId
            |_| { "failed due to missing local channel id" },

        MissingLocalConnection
            { chain_id: ChainId }
            | e | {
                format_args!("channel constructor failed due to missing connection id on chain id {0}",
                    e.chain_id)
            },

        MissingCounterpartyChannelId
            |_| { "failed due to missing counterparty channel id" },

        MissingCounterpartyConnection
            |_| { "failed due to missing counterparty connection" },

        MissingChannelOnDestination
            |_| { "missing channel on destination chain" },

        MissingChannelProof
            |_| { "missing channel proof" },

        MalformedProof
            [ ProofError ]
            |_| { "malformed proof" },

        ChannelProof
            [ RelayerError ]
            |_| { "failed to build channel proofs" },

        InvalidOrdering
            {
                channel_ordering: Ordering,
                counterparty_ordering: Ordering,
            }
            | e | {
                format_args!("channel ordering '{0}' does not match counterparty ordering '{1}'",
                    e.channel_ordering, e.counterparty_ordering)
            },

        ClientOperation
            {
                client_id: ClientId,
                chain_id: ChainId,
            }
            [ ForeignClientError ]
            | e | {
                format_args!("failed during an operation on client '{0}' hosted by chain '{1}'",
                    e.client_id, e.chain_id)
            },

        FetchSigner
            { chain_id: ChainId }
            [ RelayerError ]
            |e| { format_args!("failed while fetching the signer for destination chain '{}'", e.chain_id) },

        Query
            { chain_id: ChainId }
            [ RelayerError ]
            |e| { format_args!("failed during a query to chain '{0}'", e.chain_id) },

        ChainQuery
            { chain_id: ChainId }
            [ RelayerError ]
            |e| {
                format!("failed during a query to chain id {0}", e.chain_id)
            },

        QueryChannel
            { channel_id: ChannelId }
            [ SupervisorError ]
            |e| { format_args!("failed during a query to channel '{0}'", e.channel_id) },

        Submit
            { chain_id: ChainId }
            [ RelayerError ]
            |e| { format_args!("failed during a transaction submission step to chain '{0}'", e.chain_id) },

        HandshakeFinalize
            |_| { "continue handshake" },

        PartialOpenHandshake
            {
                state: State,
                counterparty_state: State
            }
            | e | {
                format_args!("the channel is partially open ({0}, {1})",
                    e.state, e.counterparty_state)
            },

        IncompleteChannelState
            {
                chain_id: ChainId,
                port_channel_id: PortChannelId,
            }
            | e | {
                format_args!("channel '{0}' on chain '{1}' has no counterparty channel id",
                    e.port_channel_id, e.chain_id)
            },

        ChannelAlreadyExist
            { channel_id: ChannelId }
            |e| { format_args!("channel '{}' already exist in an incompatible state", e.channel_id) },

        MismatchChannelEnds
            {
                chain_id: ChainId,
                port_channel_id: PortChannelId,
                expected_counterrparty_port_channel_id: PortChannelId,
                actual_counterrparty_port_channel_id: PortChannelId,
            }
            | e | {
                format_args!("channel '{0}' on chain '{1}' expected to have counterparty '{2}' but instead has '{3}'",
                    e.port_channel_id, e.chain_id,
                    e.expected_counterrparty_port_channel_id,
                    e.actual_counterrparty_port_channel_id)
            },

        MismatchPort
            {
                destination_chain_id: ChainId,
                destination_port_id: PortId,
                source_chain_id: ChainId,
                counterparty_port_id: PortId,
                counterparty_channel_id: ChannelId,
            }
            | e | {
                format_args!(
                    "channel open try to chain '{}' and destination port '{}' does not match \
                    the source chain '{}' counterparty port '{}' for channel '{}'",
                    e.destination_chain_id, e.destination_port_id,
                    e.source_chain_id,
                    e.counterparty_port_id,
                    e.counterparty_channel_id)
            },

        MissingEvent
            { description: String }
            | e | {
                format_args!("missing event: {}", e.description)
            },

        RetryInternal
            { reason: String }
            | e | {
                format_args!("Encountered internal error during retry: {}",
                    e.reason)
            },

        TxResponse
            { reason: String }
            | e | {
                format_args!("tx response error: {}",
                    e.reason)
            },

        InvalidEvent
            { event: IbcEvent }
            | e | {
                format_args!("channel object cannot be built from event: {}",
                    e.event)
            },

        MaxRetry
            {
                description: String,
                tries: u64,
                total_delay: Duration,
            }
            [ Self ]
            | e | {
                format_args!("error after maximum retry of {} and total delay of {}s: {}",
                    e.tries, e.total_delay.as_secs(), e.description)
            },
    }
}

impl HasExpiredOrFrozenError for ChannelErrorDetail {
    fn is_expired_or_frozen_error(&self) -> bool {
        match self {
            Self::ClientOperation(e) => e.source.is_expired_or_frozen_error(),
            _ => false,
        }
    }
}

impl HasExpiredOrFrozenError for ChannelError {
    fn is_expired_or_frozen_error(&self) -> bool {
        self.detail().is_expired_or_frozen_error()
    }
}
