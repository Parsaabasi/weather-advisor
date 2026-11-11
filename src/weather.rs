//! Open-Meteo client data types: URL building and minimal JSON extraction.
//!
//! Open-Meteo answers plain HTTP, so no TLS stack is needed. Only the few
//! numeric fields the demo needs are extracted; parsing is a scan for
//! `"key":<number>` rather than a full JSON parser, which is enough for the
//! flat responses of the forecast `current` object and the geocoding search
//! (first result only).

use core::fmt::Write as _;

/// Weather observation from the Open-Meteo `current` block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weather {
    /// Air temperature at 2 m, in tenths of °C.
    pub temp_tenths: i16,
    /// WMO weather code.
    pub code: u16,
    /// Precipitation in the last hour, in tenths of mm.
    pub precip_tenths: u16,
    /// UV index, in tenths.
    pub uv_tenths: u16,
    /// Daylight flag from the API (drives day/night scenery).
    pub is_day: bool,
}

/// Coordinates resolved from a city name, in millionths of a degree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Location {
    pub lat_micro: i32,
    pub lon_micro: i32,
}

/// Helper that formats a percent-encoded string inside a `write!` call.
struct Encoded<'a>(&'a str);

impl core::fmt::Display for Encoded<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for byte in self.0.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                (byte as char).fmt(f)?;
            } else {
                write!(f, "%{byte:02X}")?;
            }
        }
        Ok(())
    }
}

/// `GET` URL for the geocoding search of `city`, first result only.
pub fn geocoding_url(city: &str) -> heapless::String<160> {
    let mut url = heapless::String::new();
    let _ = core::write!(
        url,
        "http://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=en&format=json",
        Encoded(city)
    );
    url
}

/// `GET` URL for the current-conditions forecast at `location`.
pub fn forecast_url(location: Location) -> heapless::String<192> {
    let mut url = heapless::String::new();
    let _ = core::write!(
        url,
        "http://api.open-meteo.com/v1/forecast?latitude={}.{:03}&longitude={}.{:03}&current=temperature_2m,weather_code,precipitation,uv_index,is_day",
        location.lat_micro / 1_000_000,
        (location.lat_micro % 1_000_000).unsigned_abs(),
        location.lon_micro / 1_000_000,
        (location.lon_micro % 1_000_000).unsigned_abs(),
    );
    url
}

/// Parses the geocoding response, taking the first result's coordinates.
pub fn parse_geocoding(body: &str) -> Option<Location> {
    let results = find(body, "\"results\":[")?;
    let lat = parse_number_after(find(results, "\"latitude\":")?)?;
    let lon = parse_number_after(find(results, "\"longitude\":")?)?;
    Some(Location {
        lat_micro: (lat * 1_000_000.0) as i32,
        lon_micro: (lon * 1_000_000.0) as i32,
    })
}

/// Parses the forecast response `current` block.
pub fn parse_forecast(body: &str) -> Option<Weather> {
    let current = find(body, "\"current\":{")?;
    let temp = parse_number_after(find(current, "\"temperature_2m\":")?)?;
    let code = parse_number_after(find(current, "\"weather_code\":")?)?;
    let precip = parse_number_after(find(current, "\"precipitation\":")?)?;
    let uv = parse_number_after(find(current, "\"uv_index\":")?)?;
    let is_day = find(current, "\"is_day\":")
        .map(|rest| rest.trim_start().starts_with('1'))
        .unwrap_or(true);
    Some(Weather {
        temp_tenths: (temp * 10.0) as i16,
        code: code as u16,
        precip_tenths: (precip * 10.0) as u16,
        uv_tenths: (uv * 10.0) as u16,
        is_day,
    })
}

/// Finds `needle` in `haystack`, returning the remainder after it.
fn find<'a>(haystack: &'a str, needle: &str) -> Option<&'a str> {
    haystack.find(needle).map(|pos| &haystack[pos + needle.len()..])
}

/// Parses the number that (after an optional `:` and whitespace) starts the
/// given text; JSON numbers are `-?[0-9]+(\.[0-9]+)?([eE]...)?`.
fn parse_number_after(text: &str) -> Option<f32> {
    let text = text.trim_start();
    let text = text.strip_prefix(':').unwrap_or(text).trim_start();
    let mut end = 0;
    let bytes = text.as_bytes();
    while end < bytes.len()
        && (bytes[end].is_ascii_digit()
            || matches!(bytes[end], b'-' | b'+' | b'.' | b'e' | b'E'))
    {
        end += 1;
    }
    if end == 0 {
        return None;
    }
    text[..end].parse().ok()
}

/// Short English description of a WMO weather code, as used by Open-Meteo.
pub fn describe_code(code: u16) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80 | 81 => "Rain showers",
        82 => "Violent showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Storm w/ hail",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forecast() {
        let body = r#"{"latitude":49.19,"current_units":{"temperature_2m":"°C"},"current":{"time":"2026-11-11T13:00","interval":900,"temperature_2m":5.1,"weather_code":3,"precipitation":0.00,"uv_index":0.25,"is_day":1}}"#;
        let w = parse_forecast(body).unwrap();
        assert_eq!(w.temp_tenths, 51);
        assert_eq!(w.code, 3);
        assert_eq!(w.precip_tenths, 0);
        assert_eq!(w.uv_tenths, 2);
        assert!(w.is_day);
    }

    #[test]
    fn parses_geocoding() {
        let body = r#"{"results":[{"id":3078610,"name":"Brno","latitude":49.19522,"longitude":16.60796,"country":"Czechia"}]}"#;
        let loc = parse_geocoding(body).unwrap();
        assert_eq!(loc.lat_micro, 49_195_220);
        assert_eq!(loc.lon_micro, 16_607_960);
    }

    #[test]
    fn builds_urls() {
        assert_eq!(
            geocoding_url("New York").as_str(),
            "http://geocoding-api.open-meteo.com/v1/search?name=New%20York&count=1&language=en&format=json"
        );
        let url = forecast_url(Location {
            lat_micro: 49_195_220,
            lon_micro: 16_607_960,
        });
        assert!(url.contains("latitude=49.195"));
        assert!(url.contains("longitude=16.607"));
        assert!(url.contains("current=temperature_2m,weather_code,precipitation,uv_index"));
    }
}
