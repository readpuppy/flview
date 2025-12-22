use std::{net::Ipv4Addr, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use http::{header::HOST, HeaderMap, HeaderName, HeaderValue, Method, Request, Uri};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use tokio::{io::AsyncWriteExt, net::TcpStream, time::timeout};

fn parse_header_arg(s: &str) -> Result<(HeaderName, HeaderValue)> {
    let (name, value) = s
        .split_once(':')
        .ok_or_else(|| anyhow!("Invalid -H header (expected 'Name: Value'): {s}"))?;
    let name: HeaderName = name.trim().parse().context("Invalid header name")?;
    let value: HeaderValue = value.trim().parse().context("Invalid header value")?;
    Ok((name, value))
}

fn host_port(uri: &Uri) -> Result<(String, u16)> {
    let host = uri
        .host()
        .ok_or_else(|| anyhow!("URL is missing host: {uri}"))?
        .to_string();
    let port = uri
        .port_u16()
        .or_else(|| match uri.scheme_str() {
            Some("http") => Some(80),
            Some("https") => Some(443),
            _ => None,
        })
        .ok_or_else(|| anyhow!("Unsupported or missing URL scheme (http/https): {uri}"))?;
    Ok((host, port))
}

fn request_target(uri: &Uri) -> String {
    let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

fn connect_addr(host: &str, port: u16, resolveto: Option<Ipv4Addr>) -> String {
    let connect_host = resolveto
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| host.to_string());
    format!("{connect_host}:{port}")
}

pub async fn run(
    url: String,
    insecure: bool,
    extra_headers: Vec<String>,
    data: Option<String>,
    resolveto: Option<Ipv4Addr>,
) -> Result<()> {
    let uri: Uri = url.parse().context("Invalid URL")?;
    let scheme = uri
        .scheme_str()
        .ok_or_else(|| anyhow!("URL is missing scheme (http/https): {uri}"))?;
    if scheme != "http" && scheme != "https" {
        return Err(anyhow!("Unsupported URL scheme (http/https): {scheme}"));
    }

    let (host, port) = host_port(&uri)?;
    let addr = connect_addr(&host, port, resolveto);
    let connect_timeout = Duration::from_secs(10);

    let tcp = timeout(connect_timeout, TcpStream::connect(addr))
        .await
        .context("Connect timed out")?
        .context("Connect failed")?;

    let mut headers = HeaderMap::new();
    for h in extra_headers {
        let (k, v) = parse_header_arg(&h)?;
        headers.append(k, v);
    }

    let method = if data.is_some() {
        Method::POST
    } else {
        Method::GET
    };
    let body_bytes = data.unwrap_or_default().into_bytes();

    if scheme == "https" {
        https_request(uri, host, tcp, insecure, headers, method, body_bytes).await
    } else {
        http_request(uri, host, tcp, headers, method, body_bytes).await
    }
}

async fn http_request(
    uri: Uri,
    host: String,
    tcp: TcpStream,
    headers: HeaderMap,
    method: Method,
    body: Vec<u8>,
) -> Result<()> {
    let io = TokioIo::new(tcp);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .context("HTTP handshake failed")?;
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let mut req = Request::builder()
        .method(method)
        .uri(request_target(&uri))
        .body(Full::<Bytes>::from(body))
        .context("Failed to build request")?;

    req.headers_mut()
        .insert(HOST, HeaderValue::from_str(&host)?);
    req.headers_mut().extend(headers);

    let res = sender.send_request(req).await.context("Request failed")?;
    print_response(res).await
}

async fn https_request(
    uri: Uri,
    host: String,
    tcp: TcpStream,
    insecure: bool,
    headers: HeaderMap,
    method: Method,
    body: Vec<u8>,
) -> Result<()> {
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

    #[derive(Debug)]
    struct NoVerifier;
    impl ServerCertVerifier for NoVerifier {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> std::result::Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            vec![
                rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                rustls::SignatureScheme::ED25519,
                rustls::SignatureScheme::RSA_PSS_SHA256,
                rustls::SignatureScheme::RSA_PSS_SHA384,
                rustls::SignatureScheme::RSA_PSS_SHA512,
                rustls::SignatureScheme::RSA_PKCS1_SHA256,
                rustls::SignatureScheme::RSA_PKCS1_SHA384,
                rustls::SignatureScheme::RSA_PKCS1_SHA512,
            ]
        }
    }

    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    if insecure {
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(NoVerifier));
    }

    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));

    let server_name = ServerName::try_from(host.clone()).context("Invalid DNS name")?;
    let tls = connector
        .connect(server_name, tcp)
        .await
        .context("TLS handshake failed")?;

    let (_, session) = tls.get_ref();
    print_tls_summary(session, insecure);

    let io = TokioIo::new(tls);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .context("HTTP handshake failed")?;
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let mut req = Request::builder()
        .method(method)
        .uri(request_target(&uri))
        .body(Full::<Bytes>::from(body))
        .context("Failed to build request")?;

    req.headers_mut()
        .insert(HOST, HeaderValue::from_str(&host)?);
    req.headers_mut().extend(headers);

    let res = sender.send_request(req).await.context("Request failed")?;
    print_response(res).await
}

fn print_tls_summary(session: &rustls::ClientConnection, insecure: bool) {
    eprintln!("TLS:");
    eprintln!("  insecure (-k): {}", insecure);
    eprintln!(
        "  protocol: {}",
        session
            .protocol_version()
            .map(|v| format!("{v:?}"))
            .unwrap_or_else(|| "unknown".to_string())
    );
    eprintln!(
        "  cipher suite: {}",
        session
            .negotiated_cipher_suite()
            .map(|cs| format!("{:?}", cs.suite()))
            .unwrap_or_else(|| "unknown".to_string())
    );
    eprintln!(
        "  alpn: {}",
        session
            .alpn_protocol()
            .map(|p| String::from_utf8_lossy(p).to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    match session.peer_certificates() {
        Some(certs) => {
            eprintln!("  peer certificates: {}", certs.len());
            for (i, c) in certs.iter().enumerate() {
                eprintln!("    - [{}] {} bytes", i, c.as_ref().len());
            }
        }
        None => eprintln!("  peer certificates: none"),
    }
}

async fn print_response(res: http::Response<Incoming>) -> Result<()> {
    println!("HTTP/{:?} {}", res.version(), res.status());
    for (k, v) in res.headers().iter() {
        println!(
            "{}: {}",
            k.as_str(),
            v.to_str().unwrap_or("<non-utf8 header value>")
        );
    }

    // Drain the body so connections close cleanly.
    let mut body = res.into_body();
    while let Some(frame) = body.frame().await {
        let frame = frame.context("Failed reading response body")?;
        if let Ok(bytes) = frame.into_data() {
            tokio::io::stdout().write_all(&bytes).await.ok();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_header_arg_splits_once() {
        let (k, v) = parse_header_arg("X-Test: 123").unwrap();
        assert_eq!(k.as_str(), "x-test");
        assert_eq!(v.to_str().unwrap(), "123");
    }

    #[test]
    fn resolveto_overrides_connect_host() {
        let addr = connect_addr("example.com", 443, Some(Ipv4Addr::new(203, 0, 113, 10)));
        assert_eq!(addr, "203.0.113.10:443");
    }

    #[test]
    fn without_resolveto_connects_to_host() {
        let addr = connect_addr("example.com", 443, None);
        assert_eq!(addr, "example.com:443");
    }
}
