use std::{cmp::Ordering, error::Error};

use amtrak_api::{
    responses::{TrainState, TrainStatus},
    Client,
};

use super::{UpcomingTrain, UpcomingTrainStatus};

pub(super) struct AmtrakProvider {
    station_code: String,
    client: Client,
}

impl AmtrakProvider {
    pub(super) fn new(station_code: String) -> Self {
        let client = Client::new();

        Self {
            client,
            station_code,
        }
    }

    pub(super) async fn arrivals(&self) -> Result<Vec<UpcomingTrain>, Box<dyn Error>> {
        let Self {
            station_code,
            client,
            ..
        } = self;

        let arrivals = client
            .trains()
            .await?
            .0
            .into_iter()
            .flat_map(|(_, trains)| {
                trains.into_iter().filter(|train| {
                    train.train_state == TrainState::Active
                        || train.train_state == TrainState::Predeparture
                })
            })
            .filter_map(|train| {
                let station = train
                    .stations
                    .into_iter()
                    .find(|station| &station.code == station_code)
                    .filter(|station| (station.status == TrainStatus::Enroute));

                if let Some(station) = station {
                    Some(UpcomingTrain {
                        schedule_arrival: station.schedule_arrival,
                        destination_name: train.destination_name,
                        train_id: train.train_id,
                        status: match station.arrival {
                            None => super::UpcomingTrainStatus::Unknown,
                            Some(est_arrival) => {
                                let mins_early = station
                                    .schedule_arrival
                                    .signed_duration_since(est_arrival)
                                    .num_minutes();

                                match mins_early.cmp(&0) {
                                    Ordering::Equal => super::UpcomingTrainStatus::OnTime,
                                    Ordering::Less => match mins_early.abs().try_into() {
                                        Ok(num) => UpcomingTrainStatus::Late(num),
                                        Err(_) => UpcomingTrainStatus::Unknown,
                                    },
                                    Ordering::Greater => match mins_early.abs().try_into() {
                                        Ok(num) => UpcomingTrainStatus::Early(num),
                                        Err(_) => UpcomingTrainStatus::Unknown,
                                    },
                                }
                            }
                        },
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        Ok(arrivals)
    }
}
