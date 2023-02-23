use async_trait::async_trait;

use crate::relay::traits::packet_filter::PacketFilter;
use crate::relay::traits::types::HasRelayTypes;
use crate::std_prelude::*;

pub struct AllowAll;

#[async_trait]
impl<Relay> PacketFilter<Relay> for AllowAll
where
    Relay: HasRelayTypes,
{
    async fn should_relay_packet(
        _relay: &Relay,
        _packet: &Relay::Packet,
    ) -> Result<bool, Relay::Error> {
        Ok(true)
    }
}
