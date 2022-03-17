use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use derive_builder::Builder;
use futures::StreamExt;
use holochain_keystore::MetaLairClient;
use holochain_types::prelude::*;

use crate::types::ChainData;

pub use generated::*;

pub mod agent_info;
pub mod hdk;
mod generated;

pub struct GenerateBatch<I>
where
    I: IntoIterator<Item = ChainData>,
{
    pub agent_data: Vec<Generate<I>>,
}

pub struct Generate<I>
where
    I: IntoIterator<Item = ChainData>,
{
    pub keystore: MetaLairClient,
    pub data: I,
    pub dna_hash: DnaHash,
    pub genesis_settings: GenesisBuilder,
}

#[derive(Builder)]
pub struct Genesis {
    pub author: AgentPubKey,
    #[builder(default)]
    pub membrane_proof: Option<MembraneProof>,
    #[builder(default)]
    pub start_time: Option<SystemTime>,
}

impl<I> GenerateBatch<I>
where
    I: IntoIterator<Item = ChainData>,
{
    pub async fn make(self) -> HashMap<Arc<AgentPubKey>, Vec<Arc<Element>>> {
        let stream = self.agent_data.into_iter().map(|generate| async move {
            let Generate {
                keystore,
                data,
                dna_hash,
                mut genesis_settings,
            } = generate;
            if genesis_settings.author.is_none() {
                let author = keystore.new_sign_keypair_random().await.unwrap();
                genesis_settings.author(author);
            }
            let genesis_data = genesis_settings.build().unwrap();
            let author = genesis_data.author.clone();
            let genesis_items = genesis(dna_hash.clone(), &keystore, genesis_data).await;

            let data = data.into_iter();

            let timestamp =
                (genesis_items[2].header().timestamp() + Duration::from_micros(1)).unwrap();
            let prev_header = genesis_items[2].header_address().clone();
            let mut common = HeaderBuilderCommon {
                author: author.clone(),
                timestamp,
                header_seq: 3,
                prev_header,
            };
            let data_stream = data.map(|ChainData { header, entry }| {
                common.timestamp = (common.timestamp + Duration::from_micros(1)).unwrap();
                common.header_seq += 1;
                let header = header.build(common.clone());
                let header = HeaderHashed::from_content_sync(header);
                common.prev_header = header.to_hash();
                let ks = keystore.clone();
                async move {
                    Arc::new(Element::new(
                        SignedHeaderHashed::sign(&ks, header).await.unwrap(),
                        entry,
                    ))
                }
            });
            (
                Arc::new(author),
                futures::stream::iter(data_stream)
                    .buffer_unordered(10)
                    .collect()
                    .await,
            )
        });
        futures::stream::iter(stream)
            .buffer_unordered(10)
            .collect()
            .await
    }
}

// impl GenesisBuilder {
//     fn random_agent() -> AgentPubKey {
//         make(|u| AgentPubKey::arbitrary(u))
//     }
// }

async fn genesis(dna_hash: DnaHash, keystore: &MetaLairClient, genesis: Genesis) -> [Element; 3] {
    let Genesis {
        author,
        membrane_proof,
        start_time,
    } = genesis;
    let timestamp = start_time
        .and_then(|s| {
            s.duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|d| Timestamp::from_micros(d.as_micros() as i64))
        })
        .unwrap_or_else(|| Timestamp::now());
    let dna_header = Header::Dna(header::Dna {
        author: author.clone(),
        timestamp: timestamp.clone(),
        hash: dna_hash,
    });
    let dna_header = HeaderHashed::from_content_sync(dna_header);
    let dna_header = SignedHeaderHashed::sign(keystore, dna_header)
        .await
        .unwrap();
    let dna_header_address = dna_header.as_hash().clone();
    let dna_element = Element::new(dna_header, None);

    let timestamp = (timestamp + Duration::from_micros(1)).unwrap();

    // create the agent validation entry and add it directly to the store
    let agent_validation_header = Header::AgentValidationPkg(header::AgentValidationPkg {
        author: author.clone(),
        timestamp: timestamp.clone(),
        header_seq: 1,
        prev_header: dna_header_address,
        membrane_proof,
    });
    let agent_validation_header = HeaderHashed::from_content_sync(agent_validation_header);
    let agent_validation_header = SignedHeaderHashed::sign(keystore, agent_validation_header)
        .await
        .unwrap();
    let avh_addr = agent_validation_header.as_hash().clone();
    let avh_element = Element::new(agent_validation_header, None);

    let timestamp = (timestamp + Duration::from_micros(1)).unwrap();

    // create a agent chain element and add it directly to the store
    let agent_header = Header::Create(header::Create {
        author: author.clone(),
        timestamp,
        header_seq: 2,
        prev_header: avh_addr,
        entry_type: header::EntryType::AgentPubKey,
        entry_hash: author.clone().into(),
    });
    let agent_header = HeaderHashed::from_content_sync(agent_header);
    let agent_header = SignedHeaderHashed::sign(&keystore, agent_header)
        .await
        .unwrap();
    let agent_element = Element::new(agent_header, Some(Entry::Agent(author)));
    [dna_element, avh_element, agent_element]
}

pub fn dump_database<Kind: DbKindT>(env: &DbWrite<Kind>, name: &str) -> PathBuf {
    let mut tmp = std::env::temp_dir();
    tmp.push("test_dbs");
    std::fs::create_dir(&tmp).ok();
    tmp.push(format!("test-{}.sqlite3", name));
    println!("dumping db to {}", tmp.display());
    std::fs::write(&tmp, b"").unwrap();
    env.conn()
        .unwrap()
        .execute("VACUUM main into ?", [tmp.to_string_lossy()])
        .unwrap();
    tmp
}

// pub async fn generate<S>(num: usize, mut f: S, cell: &SweetCell, keystore: MetaLairClient)
// where
//     S: FnMut(&SourceChain, usize),
// {
//     let mut sc = SourceChain::new(
//         cell.authored_env().clone(),
//         cell.dht_env().clone(),
//         keystore,
//         cell.agent_pubkey().clone(),
//     )
//     .await
//     .unwrap();
//     for i in 0..num {
//         f(&mut sc, i);
//     }
//     build_data(cell, sc).await;
// }

// async fn build_data(cell: &SweetCell, sc: SourceChain) {
//     let s = std::time::Instant::now();
//     cell.dht_env()
//         .async_commit(move |txn| {
//             sc.scratch()
//                 .apply(|s| {
//                     for entry in s.drain_entries() {
//                         insert_entry(txn, entry).unwrap();
//                     }
//                     for (_, shh) in s.drain_zomed_headers() {
//                         let entry_hash = shh.header().entry_hash().cloned();
//                         let item = (shh.as_hash(), shh.header(), entry_hash);
//                         let ops = produce_op_lights_from_iter(vec![item].into_iter(), 1).unwrap();
//                         let timestamp = shh.header().timestamp();
//                         let header = shh.header().clone();
//                         insert_header(txn, shh.clone()).unwrap();
//                         for op in ops {
//                             let op_type = op.get_type();
//                             let op_order =
//                                 holochain_types::dht_op::OpOrder::new(op_type, timestamp);
//                             let (_, op_hash) = holochain_types::dht_op::UniqueForm::op_hash(
//                                 op_type,
//                                 header.clone(),
//                             )
//                             .unwrap();
//                             insert_op_lite_into_authored(
//                                 txn,
//                                 op,
//                                 op_hash.clone(),
//                                 op_order,
//                                 timestamp,
//                             )
//                             .unwrap();
//                         }
//                     }
//                 })
//                 .unwrap();
//             DatabaseResult::Ok(())
//         })
//         .await
//         .unwrap();

//     dbg!(s.elapsed());

//     dump_tmp(cell.dht_env());

//     dbg!(s.elapsed());
// }

// let s = std::time::Instant::now();
// let content: String =
//     rand::Rng::sample_iter(rand::thread_rng(), &rand::distributions::Alphanumeric)
//         .take(30)
//         .map(char::from)
//         .collect();
// let msg = chat::Message {
//     uuid: uuid::Uuid::new_v4().to_string(),
//     content,
// };
// let entry: AppEntryBytes = SerializedBytes::try_from(msg).unwrap().try_into().unwrap();
// let entry = Entry::App(entry);

// let entry_hash = EntryHash::with_data_sync(&entry);
// let header_builder = builder::Create {
//     entry_type: EntryType::App(AppEntryType::new(
//         0.into(),
//         0.into(),
//         EntryVisibility::Public,
//     )),
//     entry_hash,
// };
// sc.put(
//     Some(zome.clone()),
//     header_builder.clone(),
//     Some(entry.clone()),
//     ChainTopOrdering::default(),
// )
// .await
// .unwrap();
// eprintln!("{} {:?}", i, s.elapsed());
