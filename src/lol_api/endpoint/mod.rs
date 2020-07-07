//! This module provides an object for caching
//! the most recent state of an endpoint.
//! It also can track of important information
//! like how close we are to the current rate limit
//! for any number of arbitrary rate-limit buckets.
//! These are usually populated from the response headers
//! on 200 OK responses.
//! 
//! We move between states depending on the 
//! cached rate limit buckets, response type,
//! etc. so as to avoid querying endpoints which
//! are currently rate-limited, down, etc. We
//! cache the rate limits so that we can eventually
//! move to a asynchronous paradigm and use the information
//! to intelligently decide on cooldown times, especially
//! when we receive a 429 TOO MANY REQUESTS before
//! we receive a response header that says we're about to be
//! rate-limited (e.g. a full rate-limit bucket for some
//! time unit).
//! 

// external uses
use chrono::{DateTime,Utc};
use std::collections::HashMap;
use tokio::time::{Instant, Duration};


// my mods
use crate::lol_api::{Error, ErrorKind, Result};
mod id;
pub use id::{Region, Service, Id};

/// The status allows us to keep track of
/// the latent state of the endpoint based
/// on the last update. Note this is not
/// necessarily real-time knowledge of the
/// state, beczuse we can't observe that directly,
/// but we can make a best guess provided on
/// the information provided as to what state
/// we believe the endpoint to be in.
#[derive(Debug, Clone)]
pub enum Status {
    Unkown,                      // Used at initialization mostly
    Normal,                      // Go ahead and request at will
    Cooldown(CooldownState),     // The instant we started cooldown and the duration
    JustOffCooldown(Duration),   // State is unkown but we just got off a cooldown of the given duration
}

/// Describes the cooldown when the endpoint is in 
/// a cooldown state. Note that this is heuristically
/// the cooldown that we wait before trying again, not
/// the actual cooldown. A query to the endpoint after
/// the cooldown expires may still result in a 429
/// TOO MANY REQUESTS.
#[derive(Debug, Clone)]
pub struct CooldownState {
    start : Instant,       // Time we decided to enter cooldown
    duration : Duration,   // This is an estimate
}

impl CooldownState {

    /// ctor - creates a new cooldown state with the
    /// given duration. The start time is set
    /// the instant of the structs construction.
    fn new(duration : Duration)->CooldownState {
        CooldownState {
            start : Instant::now(),
            duration : duration,
        }
    }

    /// Determines if the cooldown is finished or not.
    /// Being expired does not imply the next call will succeed,
    /// but it does imply heuristically now is a good time to
    /// attempt a probe and see if we're off cooldown or need to
    /// enter another cooldown (e.g. the next status 
    /// is Status::JustOffCooldown(_))
    /// 
    /// # Return
    /// 
    /// True if expired, false otherwise
    pub fn is_expired(&self) -> bool {
        if self.time_left().is_some() { false } else { true }
    }

    /// Determines how much time is left
    /// on the cooldown
    pub fn time_left(&self) -> Option<Duration> {
        let since_started = self.start.elapsed();
        self.duration.checked_sub(since_started)
    }
}

/// Describes a single bucket for rate limiting
/// for the endpoint. E.g. the bucket could represent
/// a rate limit window with a duration of 20 seconds,
/// and in those 20 seconds we can send up to `max_count`
/// requests before being rate limited. This allows us
/// to keep track of the most recent count for this bucket
/// based on the server responses 200 OK header fields.
/// We also keep track of any potential rollover and
/// keep estimates of when we believe the most recent
/// window began.
#[derive(Debug)]
struct RateLimitBucket {
    count : u64,                   // count so far
    max_count : u64,               // max before rate limiting
    start_time : DateTime<Utc>, // estimate of the start time based on last rollover in ms
}

/// A single endpoint encapsulates our best guess
/// of an endpoints state (e.g. rate-limited, down, etc.)
/// and potentially moves between states just before a query
/// or just after a query based on the server response.
/// It is a latent representation, so it may not reflect the
/// actual server state, but represents the state as we've
/// most recently seen it based on server responses.
/// 
/// An endpoint can be a platform endpoint (e.g. na1),
/// a service (e.g. Summoner_V4), or a method (e.g. by account).
/// In this way endpoints can be organized hierarchically.
/// 
/// The general usage flow is to call `update_status_pre_query()`
/// and then send the query. 
/// If the response is 200 OK, parse the headers and update
/// call `update_buckets` on this struct. Finally,
/// regardless of the response code, call 
/// `update_status_from_response_code()` after the server responds.
/// 
#[derive(Debug)]
pub struct Endpoint {
    status : Status,                                    // deduced status of the endpoint
    rate_limit_buckets : HashMap<u64, RateLimitBucket>, // map bucket duration to limit
    last_update_time : DateTime<Utc>,
}

impl Endpoint {

    /// constructs empty endpoint with no bucket data
    /// and status is unkown and the last update timestamp is
    /// the start of the epoch.StatusCode
    /// 
    /// #Remarks
    /// 
    /// After construction we rely on the next call to
    /// update_status_from_response_code (e.g. after the next query)
    /// to call set_buckets_from_headers() and rollover the
    /// last update time and populate the buckets. Then we also need
    /// the caller to use update_status_from_response_code() so that the
    /// status is no longer `Status::Unkown`.
    pub fn new()->Endpoint {
        Endpoint {
            status : Status::Unkown,
            rate_limit_buckets : HashMap::new(),
            last_update_time : Utc::now(),
        }
    }

    /// Uses the response headers to update the rate limit buckets and cache
    /// the most recent rate limiting data. 
    /// 
    /// #Remarks 
    /// 
    /// This does not check if the data supplied to this function is newer, but will detect
    /// if we rolled over, and we'll keep track of that. That said,
    /// you should ensure that you only call this method if you know
    /// that the timestamp provided does indeed happen after the
    /// last time this endpoint was updated (e.g. self.last_update_timestamp_ms())
    /// 
    /// # Arguments
    /// 
    /// `limits` : the pairs of parsed (limit:window_length) parsed from a 200 OK header
    /// `counts` : the pairs of parsed (count:window_length) parsed from a 200 OK header
    /// `timestamp` : the timestamp (e.g. the "Date" header) of the response that generated
    ///               the `limits` and `counts` data. Should be an i64 milliseconds since the UNIX_EPOCH
    pub fn update_buckets(&mut self, limits : &[(u64,u64)], counts :  &[(u64,u64)], response_time : DateTime<Utc>) {

        // first just update rate limits
        self.rate_limit_buckets.clear(); // in the future, only update when required
        for &(limit, bucket_size) in limits {

            let bucket = self.rate_limit_buckets.entry(bucket_size)
                .or_insert(RateLimitBucket {
                    count : 0,
                    max_count : 0,
                    start_time : Utc::now(),
                });
            bucket.max_count = limit;
        }

        // set counts for existing buckets... They better exist by now
        for &(count, bucket_size) in counts {

            let bucket = self.rate_limit_buckets.get_mut(&bucket_size).unwrap();
            if bucket.count > count { //detect rollover
                bucket.start_time = response_time;
            }
            bucket.count = count;
        }

        self.last_update_time = response_time;
    }

    /// Updates endpoint status prior to sending a query.
    /// Currently just checks for an expired cooldown and transitions to just off cooldown
    pub fn update_status_pre_query(&mut self) {
        match &self.status {
            Status::Cooldown(cd_state) if cd_state.is_expired() => {
                println!("Expired!");
                self.status = Status::JustOffCooldown(cd_state.duration); //just because we expired, doesn't guarentee normal, the cooldown was a guess
            },
            _ => {}
        }
    }

    pub fn update_status_200(&mut self) {

        match &self.status {
            Status::Normal | Status::Unkown | Status::JustOffCooldown(_) => self.set_status_normal_or_cooldown(),
            _ => {}
        }
    }

    pub fn update_status_400(&mut self) {
        match &self.status {
            Status::JustOffCooldown(prev_duration) => {
                let new_cd = prev_duration.checked_mul(2).unwrap();
                self.status = Status::Cooldown(CooldownState::new(new_cd));
            },
            _ => {},
        }
    }

    /// Checks that an endpoint is ready to be queried. 
    /// If it isn't returns an error.
    /// 
    /// # Remarks
    /// 
    /// In general a valid endpoint is one in the state:
    /// * `Unkown` - haven't queried this endpoint yet, so we'll use this query as a probe
    /// * `Normal` - g2g as far as we can tell based on received responses
    /// * `JustOffCooldown` - just came off a cooldown but may potentially 429 again
    // pub fn can_query(&mut self)->bool {
    //    match &self.status {
    //        Status::Normal | Status::Unkown | Status::JustOffCooldown(_) => true,
    //        _ => false,
    //    }
    //}

    /// Gets the current status
    /// 
    /// # Return
    /// 
    /// The Status as an endpoint::Status
    pub fn status(&self) -> Status {
        self.status.clone()
    }

    pub fn error_for_status(&self) -> Result<()> {
        match &self.status {
            Status::Cooldown(_) => Err(Error::from(ErrorKind::from(ErrorKind::EndpointNotReady(self.status())))),
            _ => Ok(()),
        }
    }

    pub fn most_likely_cd(&self) -> Option<(u64, Duration)> {
        self.rate_limit_buckets.iter().map(|(k,v)| (v.max_count - v.count, Duration::from_secs(*k))).min()
    }

    pub fn force_cd(&mut self, duration : Duration) {
        self.status = Status::Cooldown(CooldownState::new(duration));
    }

    /// Gets the last time this endpoint had its buckets updated
    /// 
    /// # Return
    /// 
    /// The timestamp of the last bucket update as an i64 
    /// milliseconds since the UNIX_EPOCH
    pub fn last_update_time(&self) -> DateTime<Utc>{
        self.last_update_time.clone()
    }

    /// A convenience function that decides whether or not 
    /// we need to set ourselves to cooldown.
    /// 
    /// # Return
    /// 
    /// `None` if we don't have to cooldown, 
    /// or a `Some` containing the cooldown to use for 
    /// settign the Cooldown(_) status.
    fn should_cooldown(&self) -> Option<CooldownState> {
        for (bucket_size, bucket) in self.rate_limit_buckets.iter() {
            if bucket.count == bucket.max_count {
                //let approx_cd_time = Utc::now().checked_sub_signed(bucket.start_timestamp);
                return Some(CooldownState::new(Duration::from_secs(*bucket_size)));
            }
        }

        None
    }

    /// Convenience function that saves some typing because
    /// after a succesful query, the next state option usually could
    /// be either the normal state or the cooldown state
    /// depending on the status of the cached rate limit buckets.
    /// 
    // after update, if this was the last in a cd window,
    // set cooldowns. This will overwrite any existing or temporary
    // cooldowns set by a 400 status code from a request that
    // was sent after but returned early
    fn set_status_normal_or_cooldown(&mut self) {

        if let Some(cd_state) = self.should_cooldown() {
            println!("should cooldown: {:?}", &cd_state);
            self.status = Status::Cooldown(cd_state);
        }
        else {
            self.status = Status::Normal;
        }
    }

}