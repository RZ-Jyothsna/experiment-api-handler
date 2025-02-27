use reqwest::{header::{HeaderMap, HeaderName, HeaderValue}, Response, StatusCode};
use std::collections::HashMap;
use tokio_retry::strategy::{ExponentialBackoff, jitter};
use tokio_retry::Retry;
use pyo3::prelude::*;
use pyo3::FromPyObject;
use tokio::runtime::Runtime;
use crate::workers::*;

mod workers;
mod dynamo_db;

pub type ReqMethod = reqwest::Method;

#[derive(Default)]
pub struct ApiRespInit {
    key: String,
    status: Option<StatusCode>,
    headers: Option<HashMap<String, String>>,
    json: Option<HashMap<String, String>>,
    text: Option<String>,
    error_msg: Option<String>,
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
            // progress: resp.progress.unwrap_or(100),
            // retry_index: resp.retry_index.unwrap_or(0),
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
    auto_retry: bool,
    retry_sleep: i32,
    use_worker: bool,
}


impl ApiReqInit {
    fn new(req: ApiReq) -> Self {
        let method = match req.method.as_deref() {
            Some("POST") => ReqMethod::POST,
            Some("PUT") => ReqMethod::PUT,
            Some("PATCH") => ReqMethod::PATCH,
            Some("DELETE") => ReqMethod::DELETE,
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
            use_worker: req.use_worker.unwrap_or(false),
        }
    }
    
    async fn before_request(&mut self) {
        // self.resp = Some(ApiResp { 
        //     key: if let Some(val) = &self.key { 
        //         val.to_string() 
        //     } else {
        //         self.key = Some("".to_string());
        //         "".to_string()
        //     },
        //     ..Default::default()
        // });
        // self.save_status().await;
    }

    // If progress needed, add this function in APiResponse impl.
    // async fn update_progress(&mut self, key: String, progress: i32) {
    //     match self.resp {
    //         Some(ref mut resp) => {
    //             resp.progress = progress;
    //         }
    //         None => {
    //             self.resp = Some(ApiResp::new(ApiRespInit {
    //                 key,
    //                 progress: Some(progress),
    //                 ..Default::default()
    //             }));
    //         }
    //     }
    //     // self.save_status().await;
    // }

    // async fn save_status() {
    //  
    // }

    async fn after_request(&self) {
        // self.save_status().await;
    }

    async fn handle_response(&self, resp: Response) -> Result<ApiResp, String> {
        println!("resp: {:?}", resp);
        let mut api_resp = ApiResp::new(ApiRespInit {
            key: if let Some(val) = &self.key { val.clone() } else { "".to_string() },
            ..Default::default()
        });

        // Headers
        api_resp.headers = Some(resp.headers().iter().map(|(k, v)| {
            let value = v.to_str();
            
            if let Ok(header_val) = value {
                return (k.as_str().to_string(), header_val.to_string());
            }
            (k.as_str().to_string(), "".to_string())
        }).collect());

        // Status
        let resp_status = resp.status();
        api_resp.status = resp_status.as_u16();
    
        if resp_status == StatusCode::INTERNAL_SERVER_ERROR {
            api_resp.error_msg = Some(format!("Server Error: {resp_status}"));

            return Err("Sever Error".to_string());
        }

        let text = resp.text().await;

        if let Ok(text) = text {

            let parsed = serde_json::from_str(&text);

            if let Ok(parsed) = parsed {
                api_resp.json = Some(parsed);
            }
            
            if resp_status == StatusCode::OK {
                api_resp.text = Some(text);
            } else {
                api_resp.error_msg = Some(text);
            }
        }

        if self.use_worker {
            self.after_request().await;
        }

        Ok(api_resp)

    }

    fn get_headers(&self) -> HeaderMap {
        let mut api_headers = HeaderMap::new();
        if let Some(headers) = &self.headers {
            for (k, v) in headers {
                let key = HeaderName::try_from(k);
                if let Ok(key) = key {
                    api_headers.insert(key, HeaderValue::from_str(v).unwrap());
                }
            }
        }
        api_headers
    }

    async fn make_api_call(mut self) -> Result<ApiResp, String> {
        // TODO: Finish it.
         if self.use_worker {
            &mut self.before_request().await;
            // Post request to worker and return request_id in resp.
            let message_id = sqs::send_message("", None, self.url).await.map_err(|e| e.to_string())?;

            let resp = ApiResp::new(ApiRespInit {
                key: message_id,
                ..Default::default()
            });

            Ok(resp)

        } else {

            let client = reqwest::Client::new();

            let retry_strategy = ExponentialBackoff::from_millis(self.retry_sleep as u64)
                .map(jitter)
                .take(self.max_retries as usize);
            
            Retry::spawn(retry_strategy, || {
                
                let client = &client;
                let url = &self.url;
                let method = &self.method;
                let options = &self.options;
                let handle_resp = |res| {
                    self.handle_response(res)
                };

                let api_headers = self.get_headers();

                async move {
                    let res = match *method {
                        ReqMethod::POST => client.post(url)
                            .json(options)
                            .headers(api_headers)
                            .send()
                            .await.map_err(|e| e.to_string())?,
                        ReqMethod::PUT => client.put(url)
                            .json(options)
                            .headers(api_headers)
                            .send()
                            .await.map_err(|e| e.to_string())?,
                        ReqMethod::PATCH => client.patch(url)
                            .json(options)
                            .headers(api_headers)
                            .send()
                            .await.map_err(|e| e.to_string())?,
                        ReqMethod::DELETE => client.delete(url)
                            .json(options)
                            .headers(api_headers)
                            .send()
                            .await.map_err(|e| e.to_string())?,
                        _ => client.get(url)
                            .headers(api_headers)
                            .send()
                            .await.map_err(|e| e.to_string())?,
                    };

                    let response = handle_resp(res).await?;

                    println!("Response: {:?}", response);
                    Ok(response)
                }
            }).await
        }
    }
}

#[pyfunction]
fn get(_py: Python, req: ApiReq) -> PyResult<ApiResp> {
    let api_req = ApiReqInit::new(req);

    let rt = Runtime::new().unwrap();

    let result = rt.block_on(
        api_req.make_api_call()
    ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;

    Ok(result)
}

#[pyfunction]
fn post(_py: Python, req: ApiReq) -> PyResult<ApiResp> {
    let api_req = ApiReqInit::new(req);

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
fn api_handler(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<ApiReq>()?;
    m.add_class::<ApiResp>()?;
    m.add_function(wrap_pyfunction!(get, m)?)?;
    m.add_function(wrap_pyfunction!(post, m)?)?;
    // Add more req methods here.
    m.add_function(wrap_pyfunction!(example_fn, m)?)?;
    Ok(())
}
