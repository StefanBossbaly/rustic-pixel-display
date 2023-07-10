use super::{StateProvider, Usefulness, UsefulnessVal};
use crate::render::Render;
use anyhow::Result;
use embedded_graphics::{
    mono_font::{self, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor},
    text::Text,
    Drawable,
};
use embedded_layout::{
    layout::linear::{spacing, LinearLayout},
    prelude::{horizontal, Chain},
};
use home_assistant_rest::get::StateEnum;
use log::warn;
use parking_lot::Mutex;
use serde::Deserialize;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::{select, task::JoinHandle};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Deserialize, Debug)]
pub struct HomeTrackerConfig {
    pub home_assistant_url: String,
    pub home_assistant_bearer_token: String,
    pub person_entity_id: String,
}

#[derive(Debug, Clone, Copy)]
pub enum PersonState {
    Home,
    Away,
    Work,
    Unknown,
}

impl Usefulness for PersonState {
    fn usefulness(&self) -> UsefulnessVal {
        match self {
            PersonState::Home | PersonState::Work => UsefulnessVal::SomewhatUseful,
            PersonState::Away => UsefulnessVal::BarelyUseful,
            PersonState::Unknown => UsefulnessVal::NotUseful,
        }
    }
}

impl<D: DrawTarget<Color = Rgb888, Error = Infallible>> Render<D> for PersonState {
    fn render(&self, canvas: &mut D) -> Result<()> {
        let state_str = match self {
            PersonState::Home => "At Home",
            PersonState::Away => "Away",
            PersonState::Work => "At Work",
            PersonState::Unknown => "Unknown",
        };

        LinearLayout::vertical(Chain::new(Text::new(
            state_str,
            Point::zero(),
            MonoTextStyle::new(&mono_font::ascii::FONT_6X10, Rgb888::WHITE),
        )))
        .with_alignment(horizontal::Left)
        .with_spacing(spacing::FixedMargin(4))
        .arrange()
        .draw(canvas)
        .unwrap();

        Ok(())
    }
}

pub struct HomeAssistantTracker {
    state: Arc<Mutex<PersonState>>,
    cancel_token: CancellationToken,
    update_task_handle: Option<JoinHandle<Result<()>>>,
}

impl HomeAssistantTracker {
    pub fn new(config: HomeTrackerConfig) -> Result<Self> {
        let home_assistant_client = home_assistant_rest::Client::new(
            &config.home_assistant_url,
            &config.home_assistant_bearer_token,
        )?;

        let state_holder = Arc::new(Mutex::new(PersonState::Unknown));
        let cancel_token = CancellationToken::new();

        // Clone the shared data since it will be moved onto the task
        let task_state_holder = state_holder.clone();
        let task_cancel_token = cancel_token.clone();

        let update_task_handle: JoinHandle<Result<()>> = tokio::task::spawn(async move {
            'update_loop: loop {
                let refresh_time = tokio::time::Instant::now() + Duration::from_secs(60);

                let person_state = match home_assistant_client
                    .get_states_of_entity(&config.person_entity_id)
                    .await
                {
                    Ok(entity_state) => {
                        // Attempt to get the person's state
                        let person_state_str = if let Some(state_value) = entity_state.state {
                            if let StateEnum::String(value) = state_value {
                                Some(value)
                            } else {
                                warn!("Could not parse 'state' as str");
                                None
                            }
                        } else {
                            warn!("{}'s 'state' was not provided", config.person_entity_id);
                            None
                        };

                        match person_state_str {
                            Some(state) => match state.to_ascii_lowercase().as_str() {
                                "home" => PersonState::Home,
                                "work" => PersonState::Work,
                                "away" => PersonState::Away,
                                _ => PersonState::Unknown,
                            },
                            None => PersonState::Unknown,
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Could not acquire home assistant status for '{}' because of {}",
                            config.person_entity_id, e
                        );

                        PersonState::Unknown
                    }
                };

                *task_state_holder.lock() = person_state;

                select! {
                    _ = tokio::time::sleep_until(refresh_time) => {},
                    _ = task_cancel_token.cancelled() => break 'update_loop,
                }
            }

            Ok(())
        });

        Ok(Self {
            state: state_holder,
            cancel_token,
            update_task_handle: Some(update_task_handle),
        })
    }
}

impl StateProvider for HomeAssistantTracker {
    type State = PersonState;

    fn provide_state(&self) -> Self::State {
        *self.state.lock()
    }
}

impl Drop for HomeAssistantTracker {
    fn drop(&mut self) {
        self.cancel_token.cancel();

        if let Some(task_handle) = self.update_task_handle.take() {
            task_handle.abort();
        }
    }
}
