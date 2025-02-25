use aws_config::meta::region::RegionProviderChain;
use aws_sdk_sqs::{Client, Error};
use tokio;

pub async fn send_message(
  queue_url: &str,
  region: Option<RegionProviderChain>,
  body: impl Into<String>
) -> Result<String, Error> {
    let region_provider= match region {
      Some(provider) => {
        provider
      }
      None => {
        RegionProviderChain::default_provider()
      }
    };

    let config = aws_config::from_env().region(region_provider).load().await;
    let client = Client::new(&config);

    // Send a message
    let response = client
        .send_message()
        .queue_url(queue_url)
        .message_body(body)
        .send()
        .await?;

    Ok(response.message_id.unwrap_or("".to_string()))
}

