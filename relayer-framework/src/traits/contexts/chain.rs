use crate::traits::contexts::runtime::RuntimeContext;
use crate::traits::core::Async;
use crate::traits::message::{IbcMessage, Message};

pub trait ChainContext: RuntimeContext {
    type Height: Async;

    type Timestamp: Async;

    type Message: Message;

    type Event: Async;
}

pub trait IbcChainContext<Counterparty>: ChainContext
where
    Counterparty: ChainContext,
{
    type ClientId: Async;

    type ConnectionId: Async;

    type ChannelId: Async;

    type PortId: Async;

    type Sequence: Async;

    type IbcMessage: IbcMessage<Counterparty>;

    type IbcEvent: Async;
}
