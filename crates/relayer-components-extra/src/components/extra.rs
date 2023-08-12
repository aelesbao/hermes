use core::marker::PhantomData;

use ibc_relayer_components::chain::traits::queries::consensus_state::ConsensusStateQuerierComponent;
use ibc_relayer_components::chain::traits::queries::status::ChainStatusQuerierComponent;
use ibc_relayer_components::components::default::DefaultComponents;
use ibc_relayer_components::relay::components::message_senders::chain_sender::SendIbcMessagesToChain;
use ibc_relayer_components::relay::components::message_senders::update_client::SendIbcMessagesWithUpdateClient;
use ibc_relayer_components::relay::components::packet_relayers::general::filter_relayer::FilterRelayer;
use ibc_relayer_components::relay::components::packet_relayers::general::full_relay::FullCycleRelayer;
use ibc_relayer_components::relay::components::packet_relayers::general::lock::LockPacketRelayer;
use ibc_relayer_components::relay::components::packet_relayers::general::log::LoggerRelayer;
use ibc_relayer_components::relay::traits::auto_relayer::{
    AutoRelayerComponent, BiRelayMode, RelayMode,
};
use ibc_relayer_components::relay::traits::ibc_message_sender::{
    IbcMessageSenderComponent, MainSink,
};
use ibc_relayer_components::relay::traits::messages::update_client::UpdateClientMessageBuilderComponent;
use ibc_relayer_components::relay::traits::packet_filter::PacketFilterComponent;
use ibc_relayer_components::relay::traits::packet_relayer::PacketRelayerComponent;
use ibc_relayer_components::relay::traits::packet_relayers::ack_packet::AckPacketRelayerComponent;
use ibc_relayer_components::relay::traits::packet_relayers::receive_packet::ReceivePacketRelayerComponnent;
use ibc_relayer_components::relay::traits::packet_relayers::timeout_unordered_packet::TimeoutUnorderedPacketRelayerComponent;

use crate::batch::components::message_sender::SendMessagesToBatchWorker;
use crate::batch::types::sink::BatchWorkerSink;
use crate::relay::components::auto_relayers::parallel_bidirectional::ParallelBidirectionalRelayer;
use crate::relay::components::auto_relayers::parallel_event::ParallelEventSubscriptionRelayer;
use crate::relay::components::auto_relayers::parallel_two_way::ParallelTwoWayAutoRelay;
use crate::relay::components::packet_relayers::retry::RetryRelayer;
use crate::telemetry::components::consensus_state::ConsensusStateTelemetryQuerier;
use crate::telemetry::components::status::ChainStatusTelemetryQuerier;

pub struct ExtraComponents<BaseComponents>(pub PhantomData<BaseComponents>);

ibc_relayer_components::forward_component!(
    ChainStatusQuerierComponent,
    ExtraComponents<BaseComponents>,
    ChainStatusTelemetryQuerier<BaseComponents>,
);

ibc_relayer_components::forward_component!(
    ConsensusStateQuerierComponent,
    ExtraComponents<BaseComponents>,
    ConsensusStateTelemetryQuerier<BaseComponents>,
);

ibc_relayer_components::forward_component!(
    IbcMessageSenderComponent<MainSink>,
    ExtraComponents<BaseComponents>,
    SendMessagesToBatchWorker,
);

ibc_relayer_components::forward_component!(
    IbcMessageSenderComponent<BatchWorkerSink>,
    ExtraComponents<BaseComponents>,
    SendIbcMessagesWithUpdateClient<SendIbcMessagesToChain>,
);

ibc_relayer_components::forward_component!(
    PacketRelayerComponent,
    ExtraComponents<BaseComponents>,
    LockPacketRelayer<LoggerRelayer<FilterRelayer<RetryRelayer<FullCycleRelayer>>>>,
);

ibc_relayer_components::forward_component!(
    AutoRelayerComponent<RelayMode>,
    ExtraComponents<BaseComponents>,
    ParallelBidirectionalRelayer<ParallelEventSubscriptionRelayer>,
);

ibc_relayer_components::forward_component!(
    AutoRelayerComponent<BiRelayMode>,
    ExtraComponents<BaseComponents>,
    ParallelTwoWayAutoRelay,
);

ibc_relayer_components::forward_components!(
    ExtraComponents<BaseComponents>,
    DefaultComponents<BaseComponents>,
    [
        UpdateClientMessageBuilderComponent,
        PacketFilterComponent,
        ReceivePacketRelayerComponnent,
        AckPacketRelayerComponent,
        TimeoutUnorderedPacketRelayerComponent,
    ]
);
