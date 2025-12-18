use chrono::{Local, TimeZone};

pub fn get_local_timestring(time: i64) -> String {
    let datetime = Local
        .timestamp_millis_opt(time)
        .single()
        .unwrap_or_else(|| {
            Local
                .timestamp_millis_opt(0)
                .single()
                .unwrap_or_else(|| Local::now())
        });
    datetime.format("%H:%M:%S").to_string()
}
