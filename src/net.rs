//! HTTP client GPUI uses to load remote images (Markdown `![](https://…)`). GPUI ships none, so
//! without this every remote image fails. Requests run on a short-lived thread with `ureq`.

use std::{sync::Arc, thread, time::Duration};

use futures::channel::oneshot;
use gpui_kit::http_client::{
    AsyncBody, HttpClient, Response, Url,
    http::{HeaderValue, Request},
};

pub struct Client {
    agent: ureq::Agent,
    user_agent: HeaderValue,
}

impl Client {
    pub fn new() -> Arc<Self> {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .build()
            .into();
        Arc::new(Self {
            agent,
            user_agent: HeaderValue::from_static(concat!("Workdesk/", env!("CARGO_PKG_VERSION"))),
        })
    }
}

impl HttpClient for Client {
    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(
        &self,
        req: Request<AsyncBody>,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        let agent = self.agent.clone();
        let user_agent = self.user_agent.clone();
        let (tx, rx) = oneshot::channel();
        // Only GETs are needed (images); request bodies are ignored.
        let uri = req.uri().to_string();
        thread::spawn(move || {
            let result = (|| -> anyhow::Result<Response<AsyncBody>> {
                let mut res = agent.get(&uri).header("User-Agent", user_agent).call()?;
                let status = res.status();
                let bytes = res
                    .body_mut()
                    .with_config()
                    .limit(25 * 1024 * 1024)
                    .read_to_vec()?;
                Ok(Response::builder()
                    .status(status.as_u16())
                    .body(AsyncBody::from(bytes))?)
            })();
            let _ = tx.send(result);
        });
        Box::pin(async move { rx.await? })
    }
}
