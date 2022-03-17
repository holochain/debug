use hdk::prelude::HdkT;

use crate::types;

pub struct ArbitraryHdk;

#[allow(unused_variables)]
impl HdkT for ArbitraryHdk {
    fn get_agent_activity(
        &self,
        _get_agent_activity_input: hdk::prelude::GetAgentActivityInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::AgentActivity> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn query(
        &self,
        filter: hdk::prelude::ChainQueryFilter,
    ) -> hdk::map_extern::ExternResult<Vec<hdk::prelude::Element>> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn sign(
        &self,
        sign: hdk::prelude::Sign,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::Signature> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn sign_ephemeral(
        &self,
        sign_ephemeral: hdk::prelude::SignEphemeral,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::EphemeralSignatures> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn verify_signature(
        &self,
        verify_signature: hdk::prelude::VerifySignature,
    ) -> hdk::map_extern::ExternResult<bool> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn create(
        &self,
        create_input: hdk::prelude::CreateInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HeaderHash> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn update(
        &self,
        update_input: hdk::prelude::UpdateInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HeaderHash> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn delete(
        &self,
        delete_input: hdk::prelude::DeleteInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HeaderHash> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn hash(
        &self,
        hash_input: hdk::prelude::HashInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HashOutput> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn get(
        &self,
        get_input: Vec<hdk::prelude::GetInput>,
    ) -> hdk::map_extern::ExternResult<Vec<Option<hdk::prelude::Element>>> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn get_details(
        &self,
        get_input: Vec<hdk::prelude::GetInput>,
    ) -> hdk::map_extern::ExternResult<Vec<Option<hdk::prelude::Details>>> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn must_get_entry(
        &self,
        must_get_entry_input: hdk::prelude::MustGetEntryInput,
    ) -> hdk::map_extern::ExternResult<holochain_types::EntryHashed> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn must_get_header(
        &self,
        must_get_header_input: hdk::prelude::MustGetHeaderInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::SignedHeaderHashed> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn must_get_valid_element(
        &self,
        must_get_valid_element_input: hdk::prelude::MustGetValidElementInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::Element> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn accept_countersigning_preflight_request(
        &self,
        preflight_request: hdk::prelude::PreflightRequest,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::PreflightRequestAcceptance> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn agent_info(
        &self,
        agent_info_input: (),
    ) -> hdk::map_extern::ExternResult<hdk::prelude::AgentInfo> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn dna_info(&self, dna_info_input: ()) -> hdk::map_extern::ExternResult<hdk::prelude::DnaInfo> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn zome_info(
        &self,
        zome_info_input: (),
    ) -> hdk::map_extern::ExternResult<hdk::prelude::ZomeInfo> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn call_info(
        &self,
        call_info_input: (),
    ) -> hdk::map_extern::ExternResult<hdk::prelude::CallInfo> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn create_link(
        &self,
        create_link_input: hdk::prelude::CreateLinkInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HeaderHash> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn delete_link(
        &self,
        delete_link_input: hdk::prelude::DeleteLinkInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HeaderHash> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn get_links(
        &self,
        get_links_input: Vec<hdk::prelude::GetLinksInput>,
    ) -> hdk::map_extern::ExternResult<Vec<Vec<hdk::prelude::Link>>> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn get_link_details(
        &self,
        get_links_input: Vec<hdk::prelude::GetLinksInput>,
    ) -> hdk::map_extern::ExternResult<Vec<hdk::prelude::LinkDetails>> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn call(
        &self,
        call: Vec<hdk::prelude::Call>,
    ) -> hdk::map_extern::ExternResult<Vec<hdk::prelude::ZomeCallResponse>> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn emit_signal(
        &self,
        app_signal: hdk::prelude::AppSignal,
    ) -> hdk::map_extern::ExternResult<()> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn remote_signal(
        &self,
        remote_signal: hdk::prelude::RemoteSignal,
    ) -> hdk::map_extern::ExternResult<()> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn random_bytes(
        &self,
        number_of_bytes: u32,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::Bytes> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn sys_time(
        &self,
        sys_time_input: (),
    ) -> hdk::map_extern::ExternResult<hdk::prelude::Timestamp> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn schedule(&self, scheduled_fn: String) -> hdk::map_extern::ExternResult<()> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn sleep(&self, wake_after: std::time::Duration) -> hdk::map_extern::ExternResult<()> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn trace(&self, trace_msg: hdk::prelude::TraceMsg) -> hdk::map_extern::ExternResult<()> {
        Ok(types::make(|u| u.arbitrary()))
    }

    fn create_x25519_keypair(
        &self,
        create_x25519_keypair_input: (),
    ) -> hdk::map_extern::ExternResult<hdk::prelude::X25519PubKey> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn x_salsa20_poly1305_decrypt(
        &self,
        x_salsa20_poly1305_decrypt: hdk::prelude::XSalsa20Poly1305Decrypt,
    ) -> hdk::map_extern::ExternResult<Option<hdk::prelude::XSalsa20Poly1305Data>> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn x_salsa20_poly1305_encrypt(
        &self,
        x_salsa20_poly1305_encrypt: hdk::prelude::XSalsa20Poly1305Encrypt,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::XSalsa20Poly1305EncryptedData> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn x_25519_x_salsa20_poly1305_encrypt(
        &self,
        x_25519_x_salsa20_poly1305_encrypt: hdk::prelude::X25519XSalsa20Poly1305Encrypt,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::XSalsa20Poly1305EncryptedData> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }

    fn x_25519_x_salsa20_poly1305_decrypt(
        &self,
        x_25519_x_salsa20_poly1305_decrypt: hdk::prelude::X25519XSalsa20Poly1305Decrypt,
    ) -> hdk::map_extern::ExternResult<Option<hdk::prelude::XSalsa20Poly1305Data>> {
        // Ok(types::make(|u| u.arbitrary()))
        todo!()
    }
}
