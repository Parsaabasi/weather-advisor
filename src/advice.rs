//! Outfit, rain/snow and UV advice rules from the project plan.
//!
//! All inputs are in tenths of a unit (°C, mm, UV index) so no floating point
//! is needed after parsing.

use crate::weather::Weather;

/// Outfit advice for the current temperature, in tenths of °C.
pub fn outfit(temp_tenths: i16) -> &'static str {
    match temp_tenths {
        ..0 => "Heavy coat, gloves, hat",
        0..=99 => "Warm coat",
        100..=159 => "Lighter coat or jacket",
        160..=219 => "Light jacket or cardigan",
        _ => "Light, breathable clothes",
    }
}

/// Rain/snow advice: `None` means nothing worth mentioning.
pub fn rain_snow(weather: &Weather) -> Option<&'static str> {
    match weather.code {
        66 | 67 => Some("Freezing rain: slippery!"),
        71..=77 | 85 | 86 => Some("Snow: coat + waterproof shoes"),
        65 | 82 => Some("Heavy rain: umbrella + jacket"),
        51..=64 | 80 | 81 => Some("Take an umbrella"),
        _ if weather.precip_tenths > 0 => Some("Rain now: take an umbrella"),
        _ => None,
    }
}

/// UV protection advice, in tenths of UV index.
pub fn uv(uv_tenths: u16) -> &'static str {
    match uv_tenths {
        0..=29 => "UV low",
        30..=59 => "Sunscreen",
        60..=79 => "Sunscreen, sunglasses, shade",
        _ => "Strong sun protection, limit exposure",
    }
}

/// Hot weather reminder threshold (25 °C); the plan does not give a band for
/// "hot", so this is a deliberate interpretation.
const HOT_TEMP_TENTHS: i16 = 250;

/// Extra reminder shown in hot weather.
pub fn hot_reminder(temp_tenths: i16) -> Option<&'static str> {
    (temp_tenths >= HOT_TEMP_TENTHS).then_some("Hot: drink water, sun protection")
}

#[cfg(test)]
mod tests {
    use crate::weather::Weather;
    use super::{hot_reminder, outfit, rain_snow, uv};

    fn w(temp: i16, code: u16, precip: u16, uv: u16) -> Weather {
        Weather {
            temp_tenths: temp,
            code,
            precip_tenths: precip,
            uv_tenths: uv,
            is_day: true,
        }
    }

    #[test]
    fn outfit_bands() {
        assert_eq!(outfit(-10), "Heavy coat, gloves, hat");
        assert_eq!(outfit(0), "Warm coat");
        assert_eq!(outfit(95), "Warm coat");
        assert_eq!(outfit(100), "Lighter coat or jacket");
        assert_eq!(outfit(159), "Lighter coat or jacket");
        assert_eq!(outfit(160), "Light jacket or cardigan");
        assert_eq!(outfit(219), "Light jacket or cardigan");
        assert_eq!(outfit(220), "Light, breathable clothes");
    }

    #[test]
    fn rain_snow_rules() {
        assert_eq!(rain_snow(&w(20, 66, 0, 0)), Some("Freezing rain: slippery!"));
        assert_eq!(rain_snow(&w(20, 67, 0, 0)), Some("Freezing rain: slippery!"));
        assert_eq!(rain_snow(&w(20, 73, 0, 0)), Some("Snow: coat + waterproof shoes"));
        assert_eq!(rain_snow(&w(20, 86, 0, 0)), Some("Snow: coat + waterproof shoes"));
        assert_eq!(rain_snow(&w(20, 65, 0, 0)), Some("Heavy rain: umbrella + jacket"));
        assert_eq!(rain_snow(&w(20, 82, 0, 0)), Some("Heavy rain: umbrella + jacket"));
        assert_eq!(rain_snow(&w(20, 61, 0, 0)), Some("Take an umbrella"));
        assert_eq!(rain_snow(&w(20, 0, 5, 0)), Some("Rain now: take an umbrella"));
        assert_eq!(rain_snow(&w(20, 2, 0, 0)), None);
    }

    #[test]
    fn uv_bands() {
        assert_eq!(uv(0), "UV low");
        assert_eq!(uv(29), "UV low");
        assert_eq!(uv(30), "Sunscreen");
        assert_eq!(uv(59), "Sunscreen");
        assert_eq!(uv(60), "Sunscreen, sunglasses, shade");
        assert_eq!(uv(79), "Sunscreen, sunglasses, shade");
        assert_eq!(uv(80), "Strong sun protection, limit exposure");
    }

    #[test]
    fn hot_reminder_threshold() {
        assert_eq!(hot_reminder(249), None);
        assert!(hot_reminder(250).is_some());
    }
}
