mod tools;

use url::Url;

#[tokio::test(threaded_scheduler)]
async fn test_http_url_with_http_proxy() -> Result<(), anyhow::Error> {
    let remote_port = tools::start_hello_world_server(false).await?;
    let remote_url = format!("http://localhost:{}", remote_port);

    let proxy_port = tools::start_proxy_server(false).await?;
    let proxy_url = Url::parse(&format!("http://localhost:{}", proxy_port)).unwrap();

    let settings = attohttpc::ProxySettingsBuilder::new()
        .http_proxy(proxy_url.clone())
        .https_proxy(proxy_url)
        .build();

    let mut sess = attohttpc::Session::new();
    sess.proxy_settings(settings);

    let resp = sess.get(remote_url).danger_accept_invalid_certs(true).send().unwrap();

    assert_eq!(resp.text().unwrap(), "hello");

    Ok(())
}

#[cfg(any(feature = "tls", feature = "tls-rustls"))]
#[tokio::test(threaded_scheduler)]
async fn test_http_url_with_https_proxy() -> Result<(), anyhow::Error> {
    let remote_port = tools::start_hello_world_server(false).await?;
    let remote_url = format!("http://localhost:{}", remote_port);

    let proxy_port = tools::start_proxy_server(true).await?;
    let proxy_url = Url::parse(&format!("https://localhost:{}", proxy_port)).unwrap();

    let settings = attohttpc::ProxySettingsBuilder::new()
        .http_proxy(proxy_url.clone())
        .https_proxy(proxy_url)
        .build();

    let mut sess = attohttpc::Session::new();
    sess.proxy_settings(settings);

    let resp = sess.get(remote_url).danger_accept_invalid_certs(true).send().unwrap();

    assert_eq!(resp.text().unwrap(), "hello");

    Ok(())
}

#[cfg(any(feature = "tls", feature = "tls-rustls"))]
#[tokio::test(threaded_scheduler)]
async fn test_https_url_with_http_proxy() -> Result<(), anyhow::Error> {
    let remote_port = tools::start_hello_world_server(true).await?;
    let remote_url = format!("https://localhost:{}", remote_port);

    let proxy_port = tools::start_proxy_server(false).await?;
    let proxy_url = Url::parse(&format!("http://localhost:{}", proxy_port)).unwrap();

    let settings = attohttpc::ProxySettingsBuilder::new()
        .http_proxy(proxy_url.clone())
        .https_proxy(proxy_url)
        .build();

    let mut sess = attohttpc::Session::new();
    sess.proxy_settings(settings);

    let resp = sess.get(remote_url).danger_accept_invalid_certs(true).send().unwrap();

    assert_eq!(resp.text().unwrap(), "hello");

    Ok(())
}

#[cfg(any(feature = "tls", feature = "tls-rustls"))]
#[tokio::test(threaded_scheduler)]
async fn test_https_url_with_https_proxy() -> Result<(), anyhow::Error> {
    let remote_port = tools::start_hello_world_server(true).await?;
    let remote_url = format!("https://localhost:{}", remote_port);

    let proxy_port = tools::start_proxy_server(true).await?;
    let proxy_url = Url::parse(&format!("https://localhost:{}", proxy_port)).unwrap();

    let settings = attohttpc::ProxySettingsBuilder::new()
        .http_proxy(proxy_url.clone())
        .https_proxy(proxy_url)
        .build();

    let mut sess = attohttpc::Session::new();
    sess.proxy_settings(settings);

    let resp = sess.get(remote_url).danger_accept_invalid_certs(true).send().unwrap();

    assert_eq!(resp.text().unwrap(), "hello");

    Ok(())
}
