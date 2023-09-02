use std::error::Error;

use chrono::FixedOffset;
use log::warn;
use septa_api::{requests::ArrivalsRequest, responses::Arrivals, types::RegionalRailStop, Client};

use super::{UpcomingTrain, UpcomingTrainStatus};

pub(super) struct SeptaProvider {
    station: RegionalRailStop,
    client: Client,
}

impl SeptaProvider {
    pub(super) fn new(station: RegionalRailStop) -> Self {
        let client = Client::new();

        Self { station, client }
    }

    pub(super) async fn arrivals(&self) -> Result<Vec<UpcomingTrain>, Box<dyn Error>> {
        let Self {
            station, client, ..
        } = self;

        let response = client
            .arrivals(ArrivalsRequest {
                station: station.clone(),
                results: None,
                direction: None,
            })
            .await?;

        // Sort the arrivals
        let mut arrivals: Vec<septa_api::responses::Arrivals> = Vec::new();
        arrivals.extend(response.northbound.into_iter());
        arrivals.extend(response.southbound.into_iter());
        arrivals.sort_by(|a, b| a.sched_time.cmp(&b.sched_time));

        arrivals.into_iter().map(|train| train.try_into()).collect()
    }
}

impl TryFrom<Arrivals> for UpcomingTrain {
    type Error = Box<dyn Error>;

    fn try_from(value: Arrivals) -> Result<Self, Self::Error> {
        Ok(UpcomingTrain {
            schedule_arrival: value
                .sched_time
                .and_local_timezone(FixedOffset::east_opt(-4 * 3600).unwrap())
                .unwrap(),
            destination_name: value.destination.to_string(),
            train_id: value.train_id,
            status: if value.status == "On Time" {
                UpcomingTrainStatus::OnTime
            } else if value.status == "N/A" {
                UpcomingTrainStatus::Unknown
            } else if let Ok(mins) = value.status.trim_end_matches(" min").parse::<u32>() {
                UpcomingTrainStatus::Late(mins)
            } else {
                warn!("Unknown SEPTA train status {}", value.status);
                UpcomingTrainStatus::Unknown
            },
        })
    }
}
