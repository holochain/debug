use std::fmt::Debug;
use std::sync::Arc;

use futures::stream::StreamExt;
pub use holochain_serialized_bytes;
use holochain_serialized_bytes::SerializedBytes;
use holochain_serialized_bytes::SerializedBytesError;
use holochain_websocket::*;
use tokio::sync::mpsc;
use url2::url2;
use url2::Url2;

pub use holochain_websocket::Respond;
pub use holochain_websocket::WebsocketReceiver;
pub use holochain_websocket::WebsocketSender;

pub async fn listen<T>() -> (
    tokio::sync::oneshot::Receiver<WebsocketSender>,
    mpsc::Receiver<T>,
    Url2,
)
where
    T: TryFrom<SerializedBytes, Error = SerializedBytesError>,
    T: Debug,
    T: 'static + Send,
{
    let (tx, rx) = mpsc::channel(1000);
    let mut listener = WebsocketListener::bind(
        url2!("ws://{}:0", crate::lan_ip()),
        std::sync::Arc::new(WebsocketConfig::default()),
    )
    .await
    .unwrap();
    let addr = listener.local_addr().clone();
    let (s_tx, s_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        if let Some(Ok((send, mut recv))) = listener.next().await {
            s_tx.send(send).unwrap();
            while let Some((msg, _)) = recv.next().await {
                // Deserialize the message
                let msg: T = msg.try_into().unwrap();
                tx.send(msg).await.unwrap();
            }
        }
    });
    (s_rx, rx, addr)
}

pub async fn connect<T>(url: Url2) -> (WebsocketSender, mpsc::Receiver<(T, Respond)>)
where
    T: TryFrom<SerializedBytes, Error = SerializedBytesError>,
    T: Debug,
    T: 'static + Send,
{
    let (send, mut recv) = holochain_websocket::connect(url, Arc::new(WebsocketConfig::default()))
        .await
        .unwrap();
    let (tx, rx) = mpsc::channel(1000);
    tokio::spawn(async move {
        while let Some((msg, resp)) = recv.next().await {
            // Deserialize the message
            let msg: T = msg.try_into().unwrap();
            if let Err(_) = tx.send((msg, resp)).await {
                panic!("Failed to send incoming msg");
            }
        }
    });
    (send, rx)
}
