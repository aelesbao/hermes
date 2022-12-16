use alloc::sync::Arc;
use ibc_relayer_framework::base::one_for_all::types::chain::OfaChainWrapper;
use ibc_relayer_framework::base::one_for_all::types::runtime::OfaRuntimeWrapper;
use ibc_relayer_runtime::tokio::context::TokioRuntimeContext;

use crate::base::traits::relay::CosmosRelay;
use crate::base::types::chain::CosmosChainWrapper;

pub struct CosmosRelayWrapper<Relay: CosmosRelay> {
    pub relay: Arc<Relay>,
    pub src_chain: OfaChainWrapper<CosmosChainWrapper<Relay::SrcChain>>,
    pub dst_chain: OfaChainWrapper<CosmosChainWrapper<Relay::DstChain>>,
    pub runtime: OfaRuntimeWrapper<TokioRuntimeContext>,
}

impl<Relay: CosmosRelay> CosmosRelayWrapper<Relay> {
    pub fn new(relay: Arc<Relay>, runtime: OfaRuntimeWrapper<TokioRuntimeContext>) -> Self {
        let src_chain = OfaChainWrapper::new(CosmosChainWrapper::new(
            relay.src_chain().clone(),
            runtime.clone(),
        ));

        let dst_chain = OfaChainWrapper::new(CosmosChainWrapper::new(
            relay.dst_chain().clone(),
            runtime.clone(),
        ));

        Self {
            relay,
            src_chain,
            dst_chain,
            runtime,
        }
    }
}
