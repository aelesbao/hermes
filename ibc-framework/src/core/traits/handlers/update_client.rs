use crate::core::aliases::client::{ClientHeader, ClientState, ConsensusState};
use crate::core::traits::client::{HasAnyClientTypes, HasClientTypes};
use crate::core::traits::error::HasError;
use crate::core::traits::ibc::HasIbcTypes;
use crate::core::traits::sync::Async;

pub trait HasAnyUpdateClientHandler: HasIbcTypes + HasAnyClientTypes + HasError {
    type AnyUpdateClientHandler: AnyUpdateClientHandler<Self>;
}

pub trait AnyUpdateClientHandler<Context>: Async
where
    Context: HasIbcTypes + HasAnyClientTypes + HasError,
{
    fn check_header_and_update_state(
        context: &Context,
        client_id: &Context::ClientId,
        client_state: &Context::AnyClientState,
        new_client_header: &Context::AnyClientHeader,
    ) -> Result<(Context::AnyClientState, Context::AnyConsensusState), Context::Error>;
}

pub trait UpdateClientHandler<Context>: Async
where
    Context: HasIbcTypes + HasError,
{
    type Client: HasClientTypes;

    fn check_header_and_update_state(
        chain: &Context,
        client_id: &Context::ClientId,
        client_state: &ClientState<Self::Client>,
        new_client_header: &ClientHeader<Self::Client>,
    ) -> Result<(ClientState<Self::Client>, ConsensusState<Self::Client>), Context::Error>;
}
