use std::sync::Arc;

use holo_hash::DnaHash;
use mock_network::hdk::counter_hdk::CounterHdk;
use mock_network::hdk::GenerateHdk;
use mock_network::*;

#[tokio::main]
async fn main() {
    let keystore = holochain_keystore::test_keystore::spawn_test_keystore()
        .await
        .unwrap();

    let dna_hash: DnaHash = types::make(|u| u.arbitrary());

    let (mut hdks, _sign_jh) = GenerateHdk {
        keystore: keystore.clone(),
        dna_hash: dna_hash.clone(),
        genesis_settings: vec![GenesisBuilder::default()],
        zome_id: 0.into(),
        entry_defs: chat::entry_defs(()).unwrap(),
        time_step: std::time::Duration::from_secs(60),
        sys_start_time: std::time::SystemTime::now(),
    }
    .make()
    .await;
    tokio::task::spawn_blocking(move || {
        let counter_hdk = CounterHdk::new().wrap(Arc::new(hdks.pop().unwrap()));

        let channels = (0..4)
            .map(|_| chat::entries::channel::Channel {
                category: "any".into(),
                uuid: uuid::Uuid::new_v4().to_string(),
            })
            .collect::<Vec<_>>();

        chat::set_hdk(counter_hdk.clone());

        for channel in channels.iter().cloned() {
            let channel_input = chat::entries::channel::ChannelInput {
                name: "test".into(),
                entry: channel.clone(),
            };
            chat::entries::channel::handlers::create_channel(channel_input).unwrap();
            for _ in 0..30 {
                let content: [char; 800] = types::make(|u| u.arbitrary());
                let msg = chat::entries::message::MessageInput {
                    last_seen: chat::entries::message::LastSeen::First,
                    channel: channel.clone(),
                    entry: chat::entries::message::Message {
                        uuid: uuid::Uuid::new_v4().to_string(),
                        content: content.into_iter().collect(),
                    },
                };
                chat::entries::message::handlers::create_message(msg).unwrap();
            }
        }

        let input = chat::ListMessagesInput {
            channel: channels.first().cloned().unwrap(),
            earliest_seen: None,
            target_message_count: 70,
        };
        eprintln!("{}", counter_hdk.display());
        counter_hdk.clear();

        chat::list_messages(input).unwrap();

        eprintln!("{}", counter_hdk.display());
    })
    .await
    .unwrap();
}
