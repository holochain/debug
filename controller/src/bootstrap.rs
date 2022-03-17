use kitsune_p2p::agent_store::AgentInfoSigned;
use reqwest::Url;
use url2::Url2;

pub async fn clear_bootstrap(url: String) {
    let client = reqwest::Client::new();
    let mut body_data = Vec::new();
    kitsune_p2p::dependencies::kitsune_p2p_types::codec::rmp_encode(&mut body_data, &()).unwrap();

    let res = client
        .post(url.as_str())
        .body(body_data)
        .header("X-Op", "clear")
        .header(reqwest::header::CONTENT_TYPE, "application/octet")
        .send()
        .await
        .unwrap();

    assert!(res.status().is_success());
}

pub async fn put_info(url: Url2, info: AgentInfoSigned) {
    let client = reqwest::Client::new();
    let mut body_data = Vec::new();
    kitsune_p2p::dependencies::kitsune_p2p_types::codec::rmp_encode(&mut body_data, info).unwrap();

    let res = client
        .post(Url::from(url))
        .body(body_data)
        .header("X-Op", "put")
        .header(reqwest::header::CONTENT_TYPE, "application/octet")
        .send()
        .await
        .unwrap();

    assert!(res.status().is_success());
}
