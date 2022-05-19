use std::collections::HashMap;
use std::sync::Arc;

use super::*;
use futures::StreamExt;
use kitsune_p2p::test_util::mock_network::*;
use kitsune_p2p::wire as kwire;
use kitsune_p2p_proxy::tx2::tx2_proxy;
use kitsune_p2p_transport_quic::tx2::*;
use kitsune_p2p_types::tx2::tx2_api::*;
use kitsune_p2p_types::tx2::tx2_pool_promote::*;
use kitsune_p2p_types::tx2::tx2_utils::TxUrl;
use kitsune_p2p_types::tx2::MsgId;
use kitsune_p2p_types::*;
use tokio::sync::Semaphore;

pub struct Quic {
    pub(crate) agents: HashMap<Arc<AgentPubKey>, Tx2Ep<kwire::Wire>>,
}

impl Quic {
    pub fn spawn(
        self,
        from_kitsune_tx: FromKitsuneMockChannelTx,
        to_kitsune_rx: ToKitsuneMockChannelRx,
    ) {
        let handles = Arc::new(
            self.agents
                .iter()
                .map(|(a, ep)| (a.clone(), ep.handle().clone()))
                .collect::<HashMap<_, _>>(),
        );
        let addr_to_handle = Arc::new(
            self.agents
                .values()
                .map(|ep| {
                    let url = ep.handle().local_addr().unwrap();
                    let cert = ep.handle().local_cert();
                    (KitsuneMockAddr { cert, url }, ep.handle().clone())
                })
                .collect::<HashMap<_, _>>(),
        );
        let parallel_limit = Arc::new(Semaphore::new(240));
        tokio::spawn({
            let handles = handles.clone();
            async move {
                futures::stream::iter(self.agents.into_iter())
                    .flat_map_unordered(None, |(agent, ep)| ep.map(move |evt| (agent.clone(), evt)))
                    .for_each_concurrent(None, |(agent, evt)| {
                        let handle = &handles[&agent];
                        let cert = handle.local_cert();
                        let url = handle.local_addr().unwrap();
                        let to = KitsuneMockAddr { cert, url };
                        let from_kitsune_tx = from_kitsune_tx.clone();
                        let parallel_limit = parallel_limit.clone();
                        async move {
                            let s = std::time::Instant::now();
                            let permit = parallel_limit.acquire_owned().await.unwrap();
                            let el = s.elapsed();
                            if el.as_secs() > 1 {
                                tracing::error!("Incoming message got queued for {:?}", el);
                            }
                            tokio::spawn(async move {
                                use Tx2EpEvent::*;
                                match evt {
                                    OutgoingConnection(_) => dbg!(),
                                    IncomingConnection(_) => (),
                                    IncomingRequest(Tx2EpIncomingRequest {
                                        data,
                                        respond,
                                        url,
                                        con,
                                    }) => {
                                        let s = std::time::Instant::now();
                                        let id = next_msg_id().as_req();
                                        let from = KitsuneMockAddr {
                                            cert: con.peer_cert(),
                                            url,
                                        };

                                        let (tx, rx) = tokio::sync::oneshot::channel();
                                        let msg = KitsuneMock::request(id, to, from, data, tx);
                                        from_kitsune_tx.send(msg).await.unwrap();
                                        // TODO this makes no sense unless we are sure responses are quick.
                                        if let Ok(data) = rx.await {
                                            respond
                                                .respond(
                                                    data.into_wire(),
                                                    KitsuneTimeout::from_millis(30000),
                                                )
                                                .await
                                                .ok();
                                        }
                                        let el = s.elapsed();
                                        if el.as_secs() > 1 {
                                            tracing::warn!("Slow request {:?}", el);
                                        }
                                    }
                                    IncomingNotify(Tx2EpIncomingNotify { data, con, url }) => {
                                        let s = std::time::Instant::now();
                                        let from = KitsuneMockAddr {
                                            cert: con.peer_cert(),
                                            url,
                                        };
                                        let msg = KitsuneMock::notify(
                                            MsgId::new_notify(),
                                            to,
                                            from,
                                            data,
                                        );
                                        from_kitsune_tx.send(msg).await.unwrap();
                                        let el = s.elapsed();
                                        if el.as_secs() > 1 {
                                            tracing::warn!("Slow notify {:?}", el);
                                        }
                                    }
                                    ConnectionClosed(_) => dbg!(),
                                    Error(_) => dbg!(),
                                    Tick => dbg!(),
                                    EndpointClosed => dbg!(),
                                }
                                drop(permit);
                            });
                        }
                    })
                    .await;
            }
        });
        tokio::spawn(async move {
            while let Some(msg) = to_kitsune_rx.recv().await {
                if let Some(handle) = addr_to_handle.get(msg.from()) {
                    let con = handle
                        .get_connection(msg.to().url.clone(), KitsuneTimeout::from_millis(30_000))
                        .await
                        .unwrap();
                    let msg_type = msg.msg_id().get_type();
                    let (msg, respond) = msg.into_msg_respond();
                    match msg_type {
                        tx2::MsgIdType::Notify => {
                            con.notify(&msg, KitsuneTimeout::from_millis(30_000))
                                .await
                                .unwrap();
                        }
                        tx2::MsgIdType::Req => {
                            let r = con
                                .request(&msg, KitsuneTimeout::from_millis(30_000))
                                .await
                                .unwrap();
                            respond.unwrap().respond(r);
                        }
                        tx2::MsgIdType::Res => unreachable!(),
                    }
                }
            }
        });
    }

    pub async fn new(peer_data: impl Iterator<Item = &Arc<AgentPubKey>>) -> Self {
        let mut agents = HashMap::with_capacity(peer_data.size_hint().1.unwrap_or_default());
        for agent in peer_data {
            let conf = QuicConfig::default();
            let f = tx2_quic_adapter(conf).await.unwrap();
            let f = tx2_pool_promote(f, Default::default());
            let f = tx2_proxy(f, Default::default()).unwrap();
            let f = tx2_api::<kwire::Wire>(f, Default::default());

            let ep = f
                .bind(
                    format!("kitsune-quic://{}:0", controller::lan_ip()),
                    KitsuneTimeout::from_millis(5000),
                )
                .await
                .unwrap();
            dbg!(ep.handle().local_addr().unwrap());
            agents.insert(agent.clone(), ep);
        }

        // let ep_hnd = ep.handle().clone();
        // let _ = ep_hnd
        //     .get_connection(
        //         "kitsune-proxy://BQfCbLA_Y3CiGBUsjQxotzz-5d2u0c6kGxPFEHERE7w/kitsune-quic/h/169.254.241.77/p/56284/--",
        //         KitsuneTimeout::from_millis(1000 * 30),
        //     )
        //     .await
        //     .unwrap();
        // tokio::spawn({
        //     async move {
        //         ep.for_each_concurrent(32, move |evt| async move {
        //             use Tx2EpEvent::*;
        //             match evt {
        //                 OutgoingConnection(_) => dbg!(),
        //                 IncomingConnection(_) => dbg!(),
        //                 IncomingRequest(_) => todo!(),
        //                 IncomingNotify(_) => todo!(),
        //                 ConnectionClosed(_) => dbg!(),
        //                 Error(_) => dbg!(),
        //                 Tick => dbg!(),
        //                 EndpointClosed => dbg!(),
        //             }
        //         })
        //         .await;
        //     }
        // });
        Self { agents }
    }

    pub fn get_urls(&self) -> impl Iterator<Item = (Arc<AgentPubKey>, TxUrl)> + '_ {
        self.agents
            .iter()
            .map(|(a, ep)| (a.clone(), ep.handle().local_addr().unwrap()))
    }
}
