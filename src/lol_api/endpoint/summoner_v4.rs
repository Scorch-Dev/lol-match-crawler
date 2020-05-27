
#[derive(Debug)]
pub struct SummonerDto {
    accountId : String,    // encrypted account id
    profileIconId : i32,   // id of summoner icon for account
    revisionData : i64,    // date of last modification as epoch millis
    name : String,         // summoner name
    id : String,           // encrypted summoner id
    puuid : String,        // encrypted puuid
    summonerLevel : i64    // level of summoner
}