use crate::{app::Message, claude};
use cosmic::iced::futures::channel::mpsc::Sender;
use futures_util::SinkExt;

pub async fn claude_usage_monitoring(token: String, channel: &mut Sender<Message>) {
    log::info!("usage monitoring subscription started");

    loop {
        log::debug!("fetching usage data from claude api");
        match claude::get_usage(&token).await {
            Ok(usage) => {
                log::info!(
                    "usage data received: daily={:.0}%, weekly={:.0}%",
                    usage.five_hour.utilization,
                    usage.seven_day.utilization
                );
                let _ = channel.send(Message::UpdateUsage(usage)).await;
            }
            Err(error) => {
                if let Some(antropic_error_response) = error.antropic_error_response
                    && antropic_error_response
                        .error
                        .message
                        .contains(claude::ANTHROPIC_ERROR_AUTH_EXPIRED)
                {
                    let _ = channel.send(Message::RefreshToken).await;
                }

                let error = error.message;

                log::error!("failed to fetch usage data: {error}");
                let _ = channel.send(Message::ThrowError(error)).await;
            }
        }

        log::debug!("waiting 5 minutes before next usage check");
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
    }
}
