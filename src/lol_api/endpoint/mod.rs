
// public uses
pub mod summoner_v4;
pub use summoner_v4::SummonerDto;

#[derive(Debug)]
pub enum State {
    Normal,           // green light
    Probing,          // yellow light
    Cooldown(f64),    // red light (cooldown in ms)
}

#[derive(Debug)]
pub struct Endpoint {
    pub state : State,
}

impl Endpoint {

    pub fn new()->Endpoint{
        Endpoint { 
            state : State::Normal,
        }
    }
}