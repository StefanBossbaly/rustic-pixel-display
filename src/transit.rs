use anyhow::{anyhow, Result};
use geoutils::{Distance, Location};
use septa_api::types::RegionalRailStop;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use strum::IntoEnumIterator;

/// The amount of time the user has to be within the radius of a station to be considered at the station.
const NO_STATUS_TO_AT_STATION: Duration = Duration::from_secs(60);

#[derive(Debug)]
struct NoStatusTracker {
    /// A map of the regional rail stop to the time the user entered into the radius of the station.
    /// We use time::Instance since we need a monotonic clock and do not care about the system time.
    station_to_first_encounter: HashMap<RegionalRailStop, Instant>,
}

#[derive(Debug)]
struct TrainEncounter {
    /// The first time the user encountered the train inside the radius of the current station.
    first_encounter_inside_station: Option<Instant>,

    /// The first time the user encountered the train outside the radius of the current station.
    first_encounter_outside_station: Option<Instant>,
}

// Have to wrap in lazy_static since from_meters is not a const function.
lazy_static! {
    /// The radius around a station that a user must be within to be considered at the station.
    static ref AT_STATION_ENTER_RADIUS: Distance = Distance::from_meters(200.0);
}

/// The amount of time that a user would need to be outside a station's radius to
/// transition from AtStation to NoStatus.
const AT_STATION_TO_NO_STATUS_TIMEOUT: Duration = Duration::from_secs(180);

// Have to wrap in lazy_static since from_meters is not a const function.
lazy_static! {
    static ref AT_STATION_LEAVE_RADIUS: Distance = Distance::from_meters(200.0);
}

#[derive(Debug)]
struct StationTracker {
    /// The station the user is currently at.
    station: RegionalRailStop,

    /// A map from the unique train id to the time the user encountered the train within the radius
    /// of a station and the time the user encountered the train outside the radius of a station.
    train_id_to_first_encounter: HashMap<String, TrainEncounter>,

    /// The time the user has been outside the radius of the station.
    time_outside_station: Option<Instant>,
}

#[derive(Debug)]
struct TrainTracker {
    /// The unique train id.
    train_id: String,

    /// The time the user has been on the train.
    last_train_encounter: Instant,
}

#[derive(Debug)]
enum State {
    NoStatus(NoStatusTracker),
    AtStation(StationTracker),
    OnTrain(TrainTracker),
}

pub(crate) struct TransitLocation {
    state: State,
}

impl TransitLocation {
    fn new() -> Self {
        Self {
            state: State::NoStatus(NoStatusTracker {
                station_to_first_encounter: HashMap::new(),
            }),
        }
    }

    fn update_location(&mut self, lat: f64, lon: f64) -> Result<()> {
        // Get the monotonic time
        let now = Instant::now();
        let person_location = Location::new(lat, lon);

        match &mut self.state {
            State::NoStatus(ref mut tracker) => {
                let mut eligible_stations = Vec::new();

                // See if we are currently in any station's radius
                for station in
                    RegionalRailStop::iter().filter(|p| !matches!(p, RegionalRailStop::Unknown(_)))
                {
                    let station_lat_lon = station.lat_lon()?;
                    let station_location = Location::new(station_lat_lon.0, station_lat_lon.1);

                    if person_location
                        .is_in_circle(&station_location, AT_STATION_ENTER_RADIUS.clone())
                        .expect("is_in_circle failed")
                    {
                        match tracker.station_to_first_encounter.get(&station) {
                            Some(first_encounter) => {
                                if now - *first_encounter > NO_STATUS_TO_AT_STATION {
                                    eligible_stations.push(station);
                                }
                            }
                            None => {
                                tracker
                                    .station_to_first_encounter
                                    .insert(station.clone(), now.clone());
                            }
                        }
                    } else {
                        // We are not in the radius of the station, so remove it from the map
                        tracker.station_to_first_encounter.remove(&station);
                    }
                }

                // Iterate over eligible stations, if there are more than one, pick the closest one
                match eligible_stations.len() {
                    0 => {}
                    1 => {
                        let station = eligible_stations[0].clone();
                        self.state = State::AtStation(StationTracker {
                            station,
                            train_id_to_first_encounter: HashMap::new(),
                            time_outside_station: None,
                        });
                    }
                    _ => {
                        let mut closest_station = eligible_stations[0].clone();
                        let closest_lat_lon = closest_station.lat_lon()?;
                        let mut closest_distance: Distance = person_location
                            .distance_to(&Location::new(closest_lat_lon.0, closest_lat_lon.1))
                            .map_err(|e| anyhow!("distance_to failed: {}", e))?;

                        for station in eligible_stations {
                            let station_lat_lon = station.lat_lon()?;
                            let distance = person_location
                                .distance_to(&Location::new(station_lat_lon.0, station_lat_lon.1))
                                .map_err(|e| anyhow!("distance_to failed: {}", e))?;

                            if distance.meters() < closest_distance.meters() {
                                closest_station = station;
                                closest_distance = distance;
                            }
                        }

                        self.state = State::AtStation(StationTracker {
                            station: closest_station,
                            train_id_to_first_encounter: HashMap::new(),
                            time_outside_station: None,
                        });
                    }
                }
            }
            State::AtStation(ref mut tracker) => {
                let station_location = {
                    let station_lat_lon = tracker.station.lat_lon()?;
                    Location::new(station_lat_lon.0, station_lat_lon.1)
                };

                // See if we are still at the current location
                if person_location
                    .is_in_circle(&station_location, AT_STATION_LEAVE_RADIUS.clone())
                    .map_err(|e| anyhow!("distance_to failed: {}", e))?
                {
                    // We are still at the station, so update the time we have been outside the station
                    tracker.time_outside_station = None;
                } else {
                    // We are no longer at the station, so update the time we have been outside the station
                    match tracker.time_outside_station {
                        Some(first_left) => {
                            if now - first_left > AT_STATION_TO_NO_STATUS_TIMEOUT {
                                self.state = State::NoStatus(NoStatusTracker {
                                    station_to_first_encounter: HashMap::new(),
                                });
                            }
                        }
                        None => {
                            tracker.time_outside_station = Some(now.clone());
                        }
                    }
                }

                // TODO: See if we are in the radius of any train
            }
            State::OnTrain(_) => todo!(),
        }

        Ok(())
    }
}
