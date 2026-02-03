// SPDX-License-Identifier: MPL-2.0

use crate::claude;
use crate::claude_monitor::claude_usage_monitoring;
use cosmic::iced::{Length, Limits, Subscription, window::Id};
use cosmic::iced_winit::commands::popup::{destroy_popup, get_popup};
use cosmic::prelude::*;
use cosmic::widget;

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
#[derive(Default)]
pub struct AppModel {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    /// The popup id.
    popup: Option<Id>,
    /// Configuration data that persists between application runs.
    /// Daily usage information
    daily_usage: f32,
    weekly_usage: f32,
    /// Controls visibility of usage progress bars.
    is_usage_visible: bool,
    /// Token for accessing the API.
    access_token: claude::ClaudeCredentials,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    LoginClicked,
    LoginCompleted(claude::AnthropicTokenResponse),
    UpdateUsage(claude::ClaudeUsageResponse),
    RefreshToken,
    RefreshTokenCompleted(claude::AnthropicTokenResponse),
    GetLocalCredentials,
    ThrowError(String),
}

/// Create a COSMIC application from the app model
impl cosmic::Application for AppModel {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = ();

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "com.github.jrdx0.ClaudeApplet";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        // Construct the app model with the runtime's core.
        let app = AppModel {
            core,
            daily_usage: 0.0,
            weekly_usage: 0.0,
            is_usage_visible: false,
            ..Default::default()
        };

        // Check for saved credentials on startup
        let command = Task::done(cosmic::Action::App(Message::GetLocalCredentials));

        (app, command)
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// The applet's button in the panel will be drawn using the main view method.
    /// This view should emit messages to toggle the applet's popup window, which will
    /// be drawn using the `view_window` method.
    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button(Self::APP_ID)
            .on_press(Message::TogglePopup)
            .into()
    }

    /// The applet's popup window will be drawn using this view method. If there are
    /// multiple poups, you may match the id parameter to determine which popup to
    /// create a view for.
    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut content_list = widget::list_column().padding(2);

        if self.is_usage_visible {
            content_list = content_list.add(widget::container(
                widget::column()
                    .spacing(2)
                    .padding(2)
                    // Daily usage progress bar
                    .push(widget::text("Daily usage"))
                    .push(widget::progress_bar(0.0..=1.0, self.daily_usage / 100.0).height(6.0))
                    .push(widget::text(format!("{:.0}%", self.daily_usage)))
                    // Weekly usage progress bar
                    .push(widget::text("Weekly usage"))
                    .push(widget::progress_bar(0.0..=1.0, self.weekly_usage / 100.0).height(6.0))
                    .push(widget::text(format!("{:.0}%", self.weekly_usage))),
            ));
        } else {
            content_list = content_list.add(widget::container(
                widget::column().spacing(10).push(
                    widget::button::standard("Login")
                        .width(Length::Fill)
                        .height(40)
                        .on_press(Message::LoginClicked),
                ),
            ));
        }

        self.core.applet.popup_container(content_list).into()
    }

    /// Register subscriptions for this application.
    ///
    /// Subscriptions are long-lived async tasks running in the background which
    /// emit messages to the application through a channel. They may be conditionally
    /// activated by selectively appending to the subscription batch, and will
    /// continue to execute for the duration that they remain in the batch.
    fn subscription(&self) -> Subscription<Self::Message> {
        struct UsageMonitor;

        let mut subscriptions = vec![];

        // Only run monitoring subscription if user is logged in
        if self.is_usage_visible && !self.access_token.access_token.is_empty() {
            let access_token = self.access_token.clone();

            subscriptions.push(Subscription::run_with_id(
                std::any::TypeId::of::<UsageMonitor>(),
                cosmic::iced::stream::channel(10, move |mut channel| {
                    let token = access_token.access_token.clone();

                    async move {
                        claude_usage_monitoring(token, &mut channel).await;
                    }
                }),
            ));
        }

        Subscription::batch(subscriptions)
    }

    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime. The application will not exit until all
    /// tasks are finished.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::GetLocalCredentials => {
                log::info!("checking for local credentials");
                match claude::get_local_credentials() {
                    Ok(credentials) => {
                        log::info!("local credentials found, logging in automatically");
                        self.access_token = credentials;
                        self.is_usage_visible = true;
                    }
                    Err(error) => {
                        log::debug!("no local credentials found: {error}");
                        let _ = cosmic::Action::App(Message::ThrowError(error));
                    }
                }
            }
            Message::LoginClicked => {
                log::info!("login button clicked, starting oauth flow");
                return Task::perform(claude::open_oauth_login(), |oauth_response| {
                    match oauth_response {
                        Ok(authorization) => {
                            cosmic::Action::App(Message::LoginCompleted(authorization))
                        }
                        Err(error) => cosmic::Action::App(Message::ThrowError(error)),
                    }
                });
            }
            Message::LoginCompleted(authorization) => {
                log::info!("login completed successfully, saving credentials");
                let _ = claude::save_credentials_locally(&authorization);

                self.access_token = claude::ClaudeCredentials {
                    access_token: authorization.access_token,
                    refresh_token: authorization.refresh_token,
                };
                self.is_usage_visible = true;
                log::info!("user authenticated, monitoring will start");
            }
            Message::RefreshToken => {
                log::info!("refreshing token started");

                self.is_usage_visible = false;
                let refresh_token = self.access_token.refresh_token.clone();

                return Task::perform(
                    claude::refresh_credentials(refresh_token),
                    |refreshed_token| match refreshed_token {
                        Ok(new_credentials) => {
                            cosmic::Action::App(Message::RefreshTokenCompleted(new_credentials))
                        }
                        Err(error) => cosmic::Action::App(Message::ThrowError(error)),
                    },
                );
            }
            Message::RefreshTokenCompleted(new_credentials) => {
                log::info!("token refreshed successfully, saving new credentials");
                let _ = claude::save_credentials_locally(&new_credentials);

                self.access_token = claude::ClaudeCredentials {
                    access_token: new_credentials.access_token,
                    refresh_token: new_credentials.refresh_token,
                };
                self.is_usage_visible = true;
                log::info!("token refreshed, monitoring will start");
            }
            Message::UpdateUsage(usage_data) => {
                log::debug!(
                    "updating ui with usage data: daily={:.0}%, weekly={:.0}%",
                    usage_data.five_hour.utilization,
                    usage_data.seven_day.utilization
                );
                self.daily_usage = usage_data.five_hour.utilization;
                self.weekly_usage = usage_data.seven_day.utilization;
            }
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(1080.0);
                    get_popup(popup_settings)
                };
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::ThrowError(error) => {
                log::error!("error occurred: {error}");
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}
