
error_chain!{

    foreign_links {
        Reqwest(::reqwest::Error);
        HeaderToString(::reqwest::header::ToStrError);
        Serde(::serde::de::value::Error);
        JoinError(::tokio::task::JoinError);
    }

    errors {
        EndpointNotReady(status : crate::lol_api::endpoint::Status) {
            description("Endpoint is not in a ready state.")
            display("Endpoint in state {:?} is not ready to receive queries.", status)
        }
    }
}

impl From<crate::lol_api::endpoint::Status> for ErrorKind {
    fn from(status : crate::lol_api::endpoint::Status) -> Self {
        ErrorKind::EndpointNotReady(status)
    }
}

impl Error {

    pub fn can_retry(&self) -> bool {
        if self.retry_time().is_some() { true } else { false }
    }

    pub fn retry_time(&self) -> Option<tokio::time::Duration> {
        match self.kind() {

            // endpoint not ready implies we are on cooldown
            ErrorKind::EndpointNotReady(state) => {
                if let crate::lol_api::endpoint::Status::Cooldown(cd) = state { 
                    match cd.time_left() {
                        Some(duration) => Some(duration),
                        None => None
                    }
                } else {
                    None 
                }
            },

            // if 429, then we're rate limited but haven't yet gotten the header
            // back from the last valid request sent (e.g. header indicates count == limit)
            // We can retry again certainly at an arbitrary time, though the next will probably
            // just give us a EndpointNotReady, whereby we can adjust our wait time accordingly
            // and do a second retry
            //ErrorKind::Reqwest(err) if err.is_status() => {
            //    let status = err.status().unwrap();
            //    if status == reqwest::StatusCode::TOO_MANY_REQUESTS { Some(tokio::time::Duration::from_secs(5)) } else { None }
            //},
            _ => None
        }
    }

}