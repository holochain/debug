use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt;
pub use hdk::hdk::HdkT;
use hdk::prelude::*;
use holochain_keystore::MetaLairClient;
use holochain_types::prelude::DnaHash;
use holochain_types::share::RwShare;

use super::GenesisBuilder;

pub mod arbitrary_hdk;
pub mod counter_hdk;

pub struct GenerateHdk {
    pub keystore: MetaLairClient,
    pub dna_hash: DnaHash,
    pub genesis_settings: Vec<GenesisBuilder>,
    pub zome_id: ZomeId,
    pub entry_defs: EntryDefsCallbackResult,
}

#[derive(Clone)]
pub struct ExtractHdk {
    author: Arc<AgentPubKey>,
    ro_state: Arc<ReadOnlyState>,
    state: RwShare<State>,
}

struct ReadOnlyState {
    zome_id: ZomeId,
    entry_index: HashMap<String, (EntryDef, u8)>,
}

struct State {
    chain: Vec<Arc<Element>>,
    elements: HashMap<HeaderHash, Arc<Element>>,
    entries: HashMap<EntryHash, Arc<Element>>,
    sign: tokio::sync::mpsc::Sender<(
        HeaderHashed,
        tokio::sync::oneshot::Sender<SignedHeaderHashed>,
    )>,
}

impl State {
    fn sign_header(&mut self, header: HeaderHashed) -> SignedHeaderHashed {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sign.blocking_send((header, tx)).unwrap();
        rx.blocking_recv().unwrap()
    }
}

impl GenerateHdk {
    pub async fn make(self) -> (Vec<ExtractHdk>, tokio::task::JoinHandle<()>) {
        let Self {
            keystore,
            dna_hash,
            genesis_settings,
            zome_id,
            entry_defs,
        } = self;
        let (jh, tx) = spawn_signing(keystore.clone());
        let entry_index = match entry_defs {
            EntryDefsCallbackResult::Defs(EntryDefs(defs)) => defs
                .into_iter()
                .enumerate()
                .map(|(i, def)| {
                    let key = match &def.id {
                        EntryDefId::App(s) => s.clone(),
                        _ => unreachable!(),
                    };
                    (key, (def, i as u8))
                })
                .collect(),
        };
        let ro_state = Arc::new(ReadOnlyState {
            zome_id,
            entry_index,
        });
        let r = futures::stream::iter(genesis_settings.into_iter().map(|mut settings| {
            let dna_hash = dna_hash.clone();
            let keystore = keystore.clone();
            let ro_state = ro_state.clone();
            let sign = tx.clone();
            async move {
                if settings.author.is_none() {
                    let author = keystore.new_sign_keypair_random().await.unwrap();
                    settings.author(author);
                }
                let genesis_data = settings.build().unwrap();
                let author = Arc::new(genesis_data.author.clone());
                let chain: Vec<_> = super::genesis(dna_hash, &keystore, genesis_data)
                    .await
                    .into_iter()
                    .map(Arc::new)
                    .collect();
                let elements = chain
                    .iter()
                    .map(|el| (el.header_address().clone(), el.clone()))
                    .collect();
                let entries = chain
                    .iter()
                    .filter_map(|el| Some((el.header().entry_hash().cloned()?, el.clone())))
                    .collect();
                let state = State {
                    sign,
                    elements,
                    entries,
                    chain,
                };
                ExtractHdk {
                    author,
                    ro_state,
                    state: RwShare::new(state),
                }
            }
        }))
        .buffer_unordered(10)
        .collect()
        .await;
        (r, jh)
    }
}

impl ExtractHdk {
    pub fn make(self) -> (Arc<AgentPubKey>, Vec<Arc<Element>>) {
        match self.state.try_unwrap() {
            Ok(state) => (self.author, state.chain),
            _ => unreachable!(),
        }
    }
}

fn spawn_signing(
    keystore: MetaLairClient,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::mpsc::Sender<(
        HeaderHashed,
        tokio::sync::oneshot::Sender<SignedHeaderHashed>,
    )>,
) {
    use holochain_types::element::SignedHeaderHashedExt;
    let (tx, mut rx): (
        _,
        tokio::sync::mpsc::Receiver<(_, tokio::sync::oneshot::Sender<_>)>,
    ) = tokio::sync::mpsc::channel(100);
    let jh = tokio::task::spawn(async move {
        while let Some((h, s)) = rx.recv().await {
            s.send(SignedHeaderHashed::sign(&keystore, h).await.unwrap())
                .unwrap()
        }
    });
    (jh, tx)
}

#[allow(unused_variables)]
impl hdk::hdk::HdkT for ExtractHdk {
    fn get_agent_activity(
        &self,
        get_agent_activity_input: hdk::prelude::GetAgentActivityInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::AgentActivity> {
        todo!()
    }

    fn query(
        &self,
        filter: hdk::prelude::ChainQueryFilter,
    ) -> hdk::map_extern::ExternResult<Vec<hdk::prelude::Element>> {
        Ok(self
            .state
            .share_ref(|e| e.elements.values().map(|el| (**el).clone()).collect()))
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
        let CreateInput {
            entry_def_id,
            entry,
            ..
        } = create_input;
        let entry_def_string = match entry_def_id {
            EntryDefId::App(s) => s,
            _ => unreachable!(),
        };
        let ed = &self.ro_state.entry_index[&entry_def_string];
        let entry_type = EntryType::App(AppEntryType {
            id: ed.1.into(),
            zome_id: self.ro_state.zome_id,
            visibility: ed.0.visibility,
        });
        let entry_hash = EntryHash::with_data_sync(&entry);
        Ok(self.state.share_mut(|state| {
            let last = state.chain.last().unwrap();
            let last_header = last.header();
            let timestamp =
                (last_header.timestamp() + std::time::Duration::from_micros(1)).unwrap();
            let create = Create {
                author: last_header.author().clone(),
                timestamp,
                header_seq: last_header.header_seq() + 1,
                prev_header: last.header_address().clone(),
                entry_type,
                entry_hash: entry_hash.clone(),
            };
            let header = state.sign_header(HeaderHashed::from_content_sync(Header::Create(create)));
            let element = Arc::new(Element::new(header, Some(entry)));
            let hash = element.header_address().clone();
            state.chain.push(element.clone());
            state.elements.insert(hash.clone(), element.clone());
            state.entries.insert(entry_hash, element);
            hash
        }))
    }

    fn create_link(
        &self,
        input: hdk::prelude::CreateLinkInput,
    ) -> hdk::map_extern::ExternResult<hdk::prelude::HeaderHash> {
        let CreateLinkInput {
            base_address,
            target_address,
            tag,
            ..
        } = input;
        let zome_id = self.ro_state.zome_id;
        Ok(self.state.share_mut(|state| {
            let last = state.chain.last().unwrap();
            let last_header = last.header();
            let timestamp =
                (last_header.timestamp() + std::time::Duration::from_micros(1)).unwrap();
            let create = CreateLink {
                author: last_header.author().clone(),
                timestamp,
                header_seq: last_header.header_seq() + 1,
                prev_header: last.header_address().clone(),
                base_address,
                target_address,
                zome_id,
                tag,
            };
            let header =
                state.sign_header(HeaderHashed::from_content_sync(Header::CreateLink(create)));
            let element = Arc::new(Element::new(header, None));
            let hash = element.header_address().clone();
            state.chain.push(element.clone());
            state.elements.insert(hash.clone(), element.clone());
            hash
        }))
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
        match hash_input {
            hdk::prelude::HashInput::Entry(entry) => {
                Ok(HashOutput::Entry(EntryHash::with_data_sync(&entry)))
            }
            _ => todo!(),
        }
    }

    fn get(
        &self,
        get_input: Vec<hdk::prelude::GetInput>,
    ) -> hdk::map_extern::ExternResult<Vec<Option<hdk::prelude::Element>>> {
        Ok(self.state.share_ref(|el| {
            get_input
                .into_iter()
                .map(|i| match i.any_dht_hash.hash_type() {
                    holo_hash::hash_type::AnyDht::Entry => {
                        let h: EntryHash = i.any_dht_hash.into();
                        el.entries.get(&h).map(|el| (**el).clone())
                    }
                    holo_hash::hash_type::AnyDht::Header => {
                        let h: HeaderHash = i.any_dht_hash.into();
                        el.elements.get(&h).map(|el| (**el).clone())
                    }
                })
                .collect()
        }))
    }

    fn get_details(
        &self,
        get_input: Vec<hdk::prelude::GetInput>,
    ) -> hdk::map_extern::ExternResult<Vec<Option<hdk::prelude::Details>>> {
        Ok(self.state.share_ref(|el| {
            get_input
                .into_iter()
                .map(|i| match i.any_dht_hash.hash_type() {
                    holo_hash::hash_type::AnyDht::Entry => {
                        let h: EntryHash = i.any_dht_hash.into();
                        el.entries.get(&h).and_then(|el| {
                            Some(Details::Entry(EntryDetails {
                                entry: el.entry().clone().into_option()?,
                                headers: vec![el.signed_header().clone()],
                                rejected_headers: Default::default(),
                                deletes: Default::default(),
                                updates: Default::default(),
                                entry_dht_status: EntryDhtStatus::Live,
                            }))
                        })
                    }
                    holo_hash::hash_type::AnyDht::Header => {
                        let h: HeaderHash = i.any_dht_hash.into();
                        el.elements.get(&h).map(|el| {
                            Details::Element(ElementDetails {
                                element: (**el).clone(),
                                validation_status: ValidationStatus::Valid,
                                deletes: Default::default(),
                                updates: Default::default(),
                            })
                        })
                    }
                })
                .collect()
        }))
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
        let header_hash = crate::types::make(|u| u.arbitrary());
        let agent = self.author.clone();
        Ok(AgentInfo {
            agent_initial_pubkey: (*agent).clone(),
            agent_latest_pubkey: (*agent).clone(),
            chain_head: (header_hash, 0, Timestamp::now()),
        })
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
        Ok(self.state.share_ref(|state| {
            get_links_input
                .into_iter()
                .map(|i| {
                    let GetLinksInput {
                        base_address: base,
                        tag_prefix: tag,
                    } = i;
                    state
                        .elements
                        .values()
                        .filter_map(|element| {
                            match element.header() {
                                Header::CreateLink(CreateLink {
                                    base_address,
                                    tag: t,
                                    target_address,
                                    timestamp,
                                    ..
                                }) => {
                                    if *base_address == base {
                                        if let Some(tag) = &tag {
                                            if tag.0.len() <= t.0.len() {
                                                if t.0[..tag.0.len()] == tag.0 {
                                                    let h = match element.header() {
                                                        Header::CreateLink(cl) => cl,
                                                        _ => unreachable!(),
                                                    }
                                                    .clone();
                                                    return Some(Link {
                                                        target: target_address.clone(),
                                                        timestamp: *timestamp,
                                                        tag: t.clone(),
                                                        create_link_hash: element
                                                            .header_address()
                                                            .clone(),
                                                    });
                                                }
                                            }
                                        } else {
                                            let h = match element.header() {
                                                Header::CreateLink(cl) => cl,
                                                _ => unreachable!(),
                                            }
                                            .clone();
                                            return Some(Link {
                                                target: target_address.clone(),
                                                timestamp: *timestamp,
                                                tag: t.clone(),
                                                create_link_hash: element.header_address().clone(),
                                            });
                                        }
                                    }
                                }
                                _ => (),
                            }
                            None
                        })
                        .collect()
                })
                .collect()
        }))
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
        Ok(Timestamp::now())
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
