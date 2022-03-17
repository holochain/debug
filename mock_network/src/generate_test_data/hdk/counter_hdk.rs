use std::collections::HashMap;
use std::sync::Arc;

use hdk::prelude::HdkT;
use holochain_types::share::RwShare;

use super::arbitrary_hdk::ArbitraryHdk;

#[derive(Clone)]
pub struct CounterHdk {
    counts: RwShare<HashMap<&'static str, usize>>,
    inner: Arc<dyn HdkT>,
}

impl Default for CounterHdk {
    fn default() -> Self {
        Self {
            counts: Default::default(),
            inner: Arc::new(ArbitraryHdk),
        }
    }
}

impl CounterHdk {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn wrap(mut self, inner: Arc<dyn HdkT>) -> Self {
        self.inner = inner;
        self
    }

    pub fn display(&self) -> String {
        self.counts.share_ref(|map| {
            map.iter()
                .map(|(name, count)| format!("{}: {}\n", name, count))
                .collect()
        })
    }

    pub fn clear(&self) {
        self.counts.share_mut(|map| map.clear());
    }

    fn increment(&self, fn_name: &'static str) {
        self.counts
            .share_mut(|map| *map.entry(fn_name).or_default() += 1);
    }
}

#[allow(unused_variables)]
impl hdk::hdk::HdkT for CounterHdk {
    fn get_agent_activity(
        &self,
        get_agent_activity_input: hdk::prelude::GetAgentActivityInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::AgentActivity> {
        self.increment("get_agent_activity");
        self.inner.get_agent_activity(get_agent_activity_input)
    }

    fn query(
        &self,
        filter: hdk::prelude::ChainQueryFilter,
    ) -> hdk::map_extern::ExternResult<Vec<hdk::prelude::Element>> {
        self.increment("query");
        self.inner.query(filter)
    }

    fn sign(
        &self,
        sign: hdk::prelude::Sign,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::Signature> {
        todo!()
    }

    fn sign_ephemeral(
        &self,
        sign_ephemeral: hdk::prelude::SignEphemeral,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::EphemeralSignatures> {
        todo!()
    }

    fn verify_signature(
        &self,
        verify_signature: hdk::prelude::VerifySignature,
    ) -> hdk::map_extern::ExternResult<bool> {
        todo!()
    }

    fn create(
        &self,
        create_input: hdk::prelude::CreateInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HeaderHash> {
        self.increment("create");
        self.inner.create(create_input)
    }

    fn create_link(
        &self,
        input: hdk::prelude::CreateLinkInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HeaderHash> {
        self.increment("create_link");
        self.inner.create_link(input)
    }

    fn update(
        &self,
        update_input: hdk::prelude::UpdateInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HeaderHash> {
        todo!()
    }

    fn delete(
        &self,
        delete_input: hdk::prelude::DeleteInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HeaderHash> {
        todo!()
    }

    fn hash(
        &self,
        hash_input: hdk::prelude::HashInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HashOutput> {
        self.increment("hash");
        self.inner.hash(hash_input)
    }

    fn get(
        &self,
        get_input: Vec<hdk::prelude::GetInput>,
    ) -> hdk::map_extern::ExternResult<Vec<Option<hdk::prelude::Element>>> {
        self.increment("get");
        self.inner.get(get_input)
    }

    fn get_details(
        &self,
        get_input: Vec<hdk::prelude::GetInput>,
    ) -> hdk::map_extern::ExternResult<Vec<Option<hdk::prelude::Details>>> {
        self.increment("get_details");
        self.inner.get_details(get_input)
    }

    fn must_get_entry(
        &self,
        must_get_entry_input: hdk::prelude::MustGetEntryInput,
    ) -> hdk::map_extern::ExternResult<holochain_types::EntryHashed> {
        todo!()
    }

    fn must_get_header(
        &self,
        must_get_header_input: hdk::prelude::MustGetHeaderInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::SignedHeaderHashed> {
        todo!()
    }

    fn must_get_valid_element(
        &self,
        must_get_valid_element_input: hdk::prelude::MustGetValidElementInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::Element> {
        todo!()
    }

    fn accept_countersigning_preflight_request(
        &self,
        preflight_request: hdk::prelude::PreflightRequest,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::PreflightRequestAcceptance> {
        todo!()
    }

    fn agent_info(
        &self,
        agent_info_input: (),
    ) -> hdk::map_extern::ExternResult<hdk::prelude::AgentInfo> {
        self.increment("agent_info");
        self.inner.agent_info(agent_info_input)
    }

    fn dna_info(&self, dna_info_input: ()) -> hdk::map_extern::ExternResult<hdk::prelude::DnaInfo> {
        todo!()
    }

    fn zome_info(
        &self,
        zome_info_input: (),
    ) -> hdk::map_extern::ExternResult<hdk::prelude::ZomeInfo> {
        todo!()
    }

    fn call_info(
        &self,
        call_info_input: (),
    ) -> hdk::map_extern::ExternResult<hdk::prelude::CallInfo> {
        todo!()
    }

    fn delete_link(
        &self,
        delete_link_input: hdk::prelude::DeleteLinkInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HeaderHash> {
        todo!()
    }

    fn get_links(
        &self,
        get_links_input: Vec<hdk::prelude::GetLinksInput>,
    ) -> hdk::map_extern::ExternResult<Vec<Vec<hdk::prelude::Link>>> {
        self.increment("get_links");
        self.inner.get_links(get_links_input)
    }

    fn get_link_details(
        &self,
        get_links_input: Vec<hdk::prelude::GetLinksInput>,
    ) -> hdk::map_extern::ExternResult<Vec<hdk::prelude::LinkDetails>> {
        todo!()
    }

    fn call(
        &self,
        call: Vec<hdk::prelude::Call>,
    ) -> hdk::map_extern::ExternResult<Vec<hdk::prelude::ZomeCallResponse>> {
        todo!()
    }

    fn emit_signal(
        &self,
        app_signal: hdk::prelude::AppSignal,
    ) -> hdk::map_extern::ExternResult<()> {
        todo!()
    }

    fn remote_signal(
        &self,
        remote_signal: hdk::prelude::RemoteSignal,
    ) -> hdk::map_extern::ExternResult<()> {
        todo!()
    }

    fn random_bytes(
        &self,
        number_of_bytes: u32,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::Bytes> {
        todo!()
    }

    fn sys_time(
        &self,
        sys_time_input: (),
    ) -> hdk::map_extern::ExternResult<hdk::prelude::Timestamp> {
        self.increment("sys_time");
        self.inner.sys_time(sys_time_input)
    }

    fn schedule(&self, scheduled_fn: String) -> hdk::map_extern::ExternResult<()> {
        todo!()
    }

    fn sleep(&self, wake_after: std::time::Duration) -> hdk::map_extern::ExternResult<()> {
        todo!()
    }

    fn trace(&self, trace_msg: hdk::prelude::TraceMsg) -> hdk::map_extern::ExternResult<()> {
        todo!()
    }

    fn create_x25519_keypair(
        &self,
        create_x25519_keypair_input: (),
    ) -> hdk::map_extern::ExternResult<hdk::prelude::X25519PubKey> {
        todo!()
    }

    fn x_salsa20_poly1305_decrypt(
        &self,
        x_salsa20_poly1305_decrypt: hdk::prelude::XSalsa20Poly1305Decrypt,
    ) -> hdk::map_extern::ExternResult<Option<hdk::prelude::XSalsa20Poly1305Data>> {
        todo!()
    }

    fn x_salsa20_poly1305_encrypt(
        &self,
        x_salsa20_poly1305_encrypt: hdk::prelude::XSalsa20Poly1305Encrypt,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::XSalsa20Poly1305EncryptedData> {
        todo!()
    }

    fn x_25519_x_salsa20_poly1305_encrypt(
        &self,
        x_25519_x_salsa20_poly1305_encrypt: hdk::prelude::X25519XSalsa20Poly1305Encrypt,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::XSalsa20Poly1305EncryptedData> {
        todo!()
    }

    fn x_25519_x_salsa20_poly1305_decrypt(
        &self,
        x_25519_x_salsa20_poly1305_decrypt: hdk::prelude::X25519XSalsa20Poly1305Decrypt,
    ) -> hdk::map_extern::ExternResult<Option<hdk::prelude::XSalsa20Poly1305Data>> {
        todo!()
    }
}
