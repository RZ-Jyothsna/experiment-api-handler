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
struct ApiReqInit {
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
impl ApiReqInit {
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
        ApiReqInit {
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
pub struct ApiReq {
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


impl ApiReq {
    fn new(req: ApiReqInit) -> Self {
        // TODO: Finish this match.
        let method = match req.method.as_deref() {
            Some("GET") => ReqMethod::GET,
            Some("POST") => ReqMethod::POST,
            _ => ReqMethod::GET,
        };
        ApiReq {
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
            api_resp.json = Some(resp.json().await.unwrap());
        } else {
            api_resp.text = Some(resp.text().await.unwrap());
        }

        if self.use_worker {
            self.after_request().await;
        }

        Ok(api_resp)

    }

    async fn get(&mut self) -> Result<ApiResp, String> {
        let client = reqwest::Client::new();

        if self.use_worker {
            self.before_request().await;
        }

        let res = client.get(&self.url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

        let response = self.handle_response(res).await?;
        Ok(response)
    }

    async fn post(&mut self) -> Result<ApiResp, String> {

        println!("POST request2");

        if self.use_worker {
            self.before_request().await;
            // Post request to worker and return request_id in resp.
        }

        let client = reqwest::Client::new();

        let retry_strategy = ExponentialBackoff::from_millis(self.retry_sleep as u64)
            .map(jitter)
            .take(self.max_retries as usize);
        
        Retry::spawn(retry_strategy, || {
            println!("in spawn");
            
            let client = &client;
            let url = self.url.clone();
            let options = self.options.clone();
            let handle_resp = |res| {
                self.handle_response(res)
            };

            let mut api_headers = HeaderMap::new();

            if let Some(headers) = self.headers.clone() {
                for (k, v) in headers {
                    // TODO: Handle Error
                    api_headers.insert(HeaderName::try_from(&k).unwrap(), HeaderValue::from_str(&v).unwrap());
                }
            }

            println!("api_headers: {:?}", api_headers);

            async move {
                let res = client.post(&url)
                    .json(&options)
                    .headers(api_headers)
                    .send()
                    .await.map_err(|e| e.to_string())?;

                // TODO: Update this response in ApiReq resp.
                let response = handle_resp(res).await?;

                println!("Response: {:?}", response);
                Ok(response)
            }
        }).await
    }
}

async fn make_api_call(req: &mut ApiReq) -> Result<ApiResp, String> {
    if req.method == ReqMethod::GET {
        let resp = req.get().await?;
        Ok(resp)
    } else {
        let resp = req.post().await?;
        Ok(resp)
    }
}

#[pyfunction]
fn api_handler_fn(py: Python, req: ApiReqInit) -> PyResult<ApiResp> {
    println!("API handler called");
    let mut api_req = ApiReq::new(req);

    // TODO: Handle this error.
    let rt = Runtime::new().unwrap();

    let result = rt.block_on(
        make_api_call(&mut api_req)
    ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;

    Ok(result)
}

#[pyfunction]
pub fn example_fn() {
    println!("Example function called");
}

#[pymodule]
fn api_handler(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<ApiReqInit>()?;
    m.add_class::<ApiResp>()?;
    m.add_function(wrap_pyfunction!(api_handler_fn, m)?)?;
    m.add_function(wrap_pyfunction!(example_fn, m)?)?;
    Ok(())
}
