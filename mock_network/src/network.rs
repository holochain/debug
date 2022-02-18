use holochain_p2p::mock_network::*;

pub struct MockNetwork {
    mock: HolochainP2pMockChannel,
}

impl MockNetwork {
    pub(crate) fn new(mock: HolochainP2pMockChannel) -> Self {
        Self { mock }
    }

    pub async fn next(
        &mut self,
    ) -> Option<(
        AddressedHolochainP2pMockMsg,
        Option<HolochainP2pMockRespond>,
    )> {
        self.mock.next().await
    }
}
