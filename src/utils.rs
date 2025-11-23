use chrono::{Local, TimeZone};

pub fn get_local_timestring(time: i64) -> String {
    let datetime = Local.timestamp_millis_opt(time).unwrap();
    datetime.format("%H:%M:%S").to_string()
}
