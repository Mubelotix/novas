use std::time::{SystemTime, UNIX_EPOCH};

const PARIS_LONGITUDE_DEGREES: f64 = 2.3522;
const PARIS_LONGITUDE_HOURS: f64 = PARIS_LONGITUDE_DEGREES / 15.0;
const UNIX_EPOCH_JD: f64 = 2440587.5;
const DELTA_T_SECONDS: f64 = 69.184;

fn main() {
	let now = SystemTime::now();
	let seconds_since_epoch = now
		.duration_since(UNIX_EPOCH)
		.expect("system time is before the Unix epoch")
		.as_secs_f64();

	let jd_utc = UNIX_EPOCH_JD + seconds_since_epoch / 86400.0;
	let jd_ut1 = jd_utc;
	let jd_tt = jd_utc + DELTA_T_SECONDS / 86400.0;
	let jd_high = jd_ut1.floor();
	let jd_low = jd_ut1 - jd_high;

	let mut sun = unsafe { core::mem::zeroed::<novas::object>() };
	sun.type_ = 0;
	sun.number = 10;

	let mut sun_ra = 0.0;
	let mut sun_dec = 0.0;
	let mut sun_dis = 0.0;
	let sun_status = unsafe { novas::app_planet(jd_tt, &mut sun, 1, &mut sun_ra, &mut sun_dec, &mut sun_dis) };
	assert_eq!(sun_status, 0, "app_planet failed");

	let mut gast = 0.0;
	let sidereal_status = unsafe { novas::sidereal_time(jd_high, jd_low, DELTA_T_SECONDS, 1, 1, 0, &mut gast) };
	assert_eq!(sidereal_status, 0, "sidereal_time failed");

	let utc_hours = (seconds_since_epoch / 3600.0) % 24.0;
	let paris_mean_solar_time = normalize_hours(utc_hours + PARIS_LONGITUDE_HOURS);
	let paris_apparent_solar_time = normalize_hours(12.0 + gast + PARIS_LONGITUDE_HOURS - sun_ra);
	let equation_of_time_minutes = signed_hours_difference(paris_apparent_solar_time, paris_mean_solar_time) * 60.0;

	println!("UTC now: {}", format_hours(utc_hours));
	println!("Paris mean time: {}", format_hours(paris_mean_solar_time));
	println!("Paris apparent solar time: {}", format_hours(paris_apparent_solar_time));
	println!("Equation of time: {equation_of_time_minutes:+.2} minutes");
}

fn normalize_hours(hours: f64) -> f64 {
	hours.rem_euclid(24.0)
}

fn signed_hours_difference(later: f64, earlier: f64) -> f64 {
	let mut diff = later - earlier;
	if diff > 12.0 {
		diff -= 24.0;
	} else if diff < -12.0 {
		diff += 24.0;
	}
	diff
}

fn format_hours(hours: f64) -> String {
	let total_seconds = (normalize_hours(hours) * 3600.0).round() as u32;
	let hour = (total_seconds / 3600) % 24;
	let minute = (total_seconds % 3600) / 60;
	let second = total_seconds % 60;
	format!("{hour:02}:{minute:02}:{second:02}")
}