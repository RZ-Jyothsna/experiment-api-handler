#![allow(unused)]
use reqwest::{header::{self, HeaderMap, HeaderName, HeaderValue}, Error, Response, StatusCode};
use serde::Deserialize;
use std::collections::HashMap;
use tokio_retry::strategy::{ExponentialBackoff, jitter};
use tokio_retry::Retry;
use pyo3::prelude::*;
use pyo3::{types::PyString, FromPyObject};
use tokio::runtime::Runtime;

pub type ReqMethod = reqwest::Method;

#[derive(Default)]
pub struct ApiRespInit {
    key: String,
    status: Option<StatusCode>,
    headers: Option<HashMap<String, String>>,
    json: Option<HashMap<String, String>>,
    text: Option<String>,
    error_msg: Option<String>,
    progress: Option<i32>,
    retry_index: Option<i32>,
}

#[pyclass]
#[derive(Default, Debug, FromPyObject)]
pub struct ApiResp {
    #[pyo3(get, set)]
    key: String,
    #[pyo3(get, set)]
    status: u16,
    #[pyo3(get, set)]
    headers: Option<HashMap<String, String>>,
    #[pyo3(get, set)]
    json: Option<HashMap<String, String>>,
    #[pyo3(get, set)]
    text: Option<String>,
    #[pyo3(get, set)]
    error_msg: Option<String>,
    #[pyo3(get, set)]
    progress: i32,
    //Can remove this.
    #[pyo3(get, set)]
    retry_index: i32,
}

impl ApiResp {
    fn new(resp: ApiRespInit) -> Self {
        ApiResp {
            key: resp.key,
            status: resp.status.unwrap_or(StatusCode::OK).as_u16(),
            headers: resp.headers,
            json: resp.json,
            text: resp.text,
            error_msg: resp.error_msg,
            progress: resp.progress.unwrap_or(100),
            retry_index: resp.retry_index.unwrap_or(0),
        }
    }
}

#[pyclass]
#[derive(Default, FromPyObject)]
struct ApiReq {
    #[pyo3(get, set)]
    pub url: String,
    #[pyo3(get, set)]
    pub options: Option<HashMap<String, String>>,
    #[pyo3(get, set)]
    pub method: Option<String>,
    #[pyo3(get, set)]
    pub headers: Option<HashMap<String, String>>,
    #[pyo3(get, set)]
    pub key: Option<String>,
    #[pyo3(get, set)]
    pub worker_group: Option<String>,
    #[pyo3(get, set)]
    pub max_retries: Option<i8>,
    #[pyo3(get, set)]
    pub retry_index: Option<i8>,
    #[pyo3(get, set)]
    pub auto_retry: Option<bool>,
    #[pyo3(get, set)]
    pub retry_sleep: Option<i32>,
    #[pyo3(get, set)]
    pub use_worker: Option<bool>,
}

#[pymethods]
impl ApiReq {
    #[new]
    fn new(
        url: String,
        options: Option<HashMap<String, String>>,
        method: Option<String>,
        headers: Option<HashMap<String, String>>,
        key: Option<String>,
        worker_group: Option<String>,
        max_retries: Option<i8>,
        retry_index: Option<i8>,
        auto_retry: Option<bool>,
        retry_sleep: Option<i32>,
        use_worker: Option<bool>,
    ) -> Self {
        ApiReq {
            url,
            options,
            method,
            headers,
            key,
            worker_group,
            max_retries,
            retry_index,
            auto_retry,
            retry_sleep,
            use_worker,
        }
    }
}

#[derive(Default, Debug)]
pub struct ApiReqInit {
    url: String,
    options: Option<HashMap<String, String>>,
    method: ReqMethod,
    headers: Option<HashMap<String, String>>,
    key: Option<String>,
    worker_group: String,
    max_retries: i8,
    // not needed for tokio-retry
    retry_index: i8,
    auto_retry: bool,
    retry_sleep: i32,
    resp: Option<ApiResp>,
    use_worker: bool,
}


impl ApiReqInit {
    fn new(req: ApiReq) -> Self {
        // TODO: Finish this match.
        let method = match req.method.as_deref() {
            Some("GET") => ReqMethod::GET,
            Some("POST") => ReqMethod::POST,
            _ => ReqMethod::GET,
        };
        ApiReqInit {
            url: req.url,
            options: req.options,
            method,
            headers: req.headers,
            key: req.key,
            worker_group: req.worker_group.unwrap_or("".to_string()),
            max_retries: req.max_retries.unwrap_or(10),
            auto_retry: req.auto_retry.unwrap_or(true),
            retry_sleep: req.retry_sleep.unwrap_or(10),
            resp: None,
            use_worker: req.use_worker.unwrap_or(false),
            retry_index: req.retry_index.unwrap_or(0),
        }
    }
    
    async fn before_request(&mut self) {
        self.resp = Some(ApiResp { 
            key: if let Some(val) = &self.key { 
                val.to_string() 
            } else {
                self.key = Some("".to_string());
                "".to_string()
            },
            ..Default::default()
        });
        // self.save_status().await;
    }

    async fn update_progress(&mut self, key: String, progress: i32) {
        match self.resp {
            Some(ref mut resp) => {
                resp.progress = progress;
            }
            None => {
                self.resp = Some(ApiResp::new(ApiRespInit {
                    key,
                    progress: Some(progress),
                    ..Default::default()
                }));
            }
        }
        // self.save_status().await;
    }

    // async fn save_status() {
    //  
    // }

    async fn after_request(&self) {
        // self.save_status().await;
    }

    async fn handle_response(&self, resp: Response) -> Result<ApiResp, String> {
        // Adding new ApiResp, instead of overwriting the resp in ApiReq. (Not sure if this is the best way)
        let mut api_resp = ApiResp::new(ApiRespInit {
            key: if let Some(val) = &self.key { val.clone() } else { "".to_string() },
            ..Default::default()
        });

        let resp_status = resp.status();

        api_resp.headers = Some(resp.headers().iter().map(|(k, v)| (
            // TODO: handle Error
            k.as_str().to_string(), v.to_str().unwrap().to_string()
        )).collect());
        api_resp.status = resp_status.as_u16();
    
        if resp_status == StatusCode::INTERNAL_SERVER_ERROR {
            api_resp.error_msg = Some(format!("Server Error: {resp_status}"));

            return Err("Internal Sever Error".to_string());
        }

        if resp_status == 200 {
            // TODO: handle Error
            let json = resp.json().await.map_err(|e| e.to_string());
            match json {
                Ok(json) => {
                    api_resp.json = Some(json);
                }
                Err(e) => {
                    api_resp.error_msg = Some(format!("Error: {e}"));
                }
            }
        } else {
            let text = resp.text().await.map_err(|e| e.to_string());
            match text {
                Ok(text) => {
                    api_resp.text = Some(text);
                }
                Err(e) => {
                    api_resp.error_msg = Some(format!("Error: {e}"));
                }
            }
        }

        if self.use_worker {
            self.after_request().await;
        }

        Ok(api_resp)

    }

    fn get_headers(&self) -> HeaderMap {
        let mut api_headers = HeaderMap::new();
        if let Some(headers) = &self.headers.clone() {
            for (k, v) in headers {
                // TODO: Handle Error
                api_headers.insert(HeaderName::try_from(k).unwrap(), HeaderValue::from_str(v).unwrap());
            }
        }
        api_headers
    }

    async fn make_api_call(&mut self) -> Result<ApiResp, String> {
         if self.use_worker {
            self.before_request().await;
            // Post request to worker and return request_id in resp.
        }

        let client = reqwest::Client::new();

        let retry_strategy = ExponentialBackoff::from_millis(self.retry_sleep as u64)
            .map(jitter)
            .take(self.max_retries as usize);
        
        Retry::spawn(retry_strategy, || {
            
            let client = &client;
            let url = self.url.clone();
            let method = self.method.clone();
            let options = self.options.clone();
            let handle_resp = |res| {
                self.handle_response(res)
            };

            let mut api_headers = self.get_headers();

            println!("method: {:?}", method);

            async move {
                let res = match method {
                    ReqMethod::POST => client.post(&url)
                        .json(&options)
                        .headers(api_headers)
                        .send()
                        .await.map_err(|e| e.to_string())?,
                    ReqMethod::PUT => client.put(&url)
                        .json(&options)
                        .headers(api_headers)
                        .send()
                        .await.map_err(|e| e.to_string())?,
                    ReqMethod::PATCH => client.patch(&url)
                        .json(&options)
                        .headers(api_headers)
                        .send()
                        .await.map_err(|e| e.to_string())?,
                    ReqMethod::DELETE => client.delete(&url)
                        .json(&options)
                        .headers(api_headers)
                        .send()
                        .await.map_err(|e| e.to_string())?,
                    _ => client.get(&url)
                        .headers(api_headers)
                        .send()
                        .await.map_err(|e| e.to_string())?,
                };

                // TODO: Update this response in ApiReq resp.
                let response = handle_resp(res).await?;

                println!("Response: {:?}", response);
                Ok(response)
            }
        }).await
    }
}

#[pyfunction]
fn get(py: Python, req: ApiReq) -> PyResult<ApiResp> {
    let mut api_req = ApiReqInit::new(req);

    let rt = Runtime::new().unwrap();

    let result = rt.block_on(
        api_req.make_api_call()
    ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;

    Ok(result)
}

#[pyfunction]
fn post(py: Python, req: ApiReq) -> PyResult<ApiResp> {
    let mut api_req = ApiReqInit::new(req);

    let rt = Runtime::new().unwrap();

    let result = rt.block_on(
        api_req.make_api_call()
    ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;

    Ok(result)
}

#[pyfunction]
pub fn example_fn() {
    println!("Example function called");
}

#[pymodule]
fn api_handler(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<ApiReq>()?;
    m.add_class::<ApiResp>()?;
    m.add_function(wrap_pyfunction!(get, m)?)?;
    m.add_function(wrap_pyfunction!(post, m)?)?;
    m.add_function(wrap_pyfunction!(example_fn, m)?)?;
    Ok(())
}
