use std::env;
use std::error::Error;
use std::ffi::c_char;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

const DEFAULT_YEAR: i32 = 2026;
const SAMPLE_HOUR_UTC: f64 = 12.0;
const DELTA_T_SECONDS: f64 = 69.184;

const WIDTH: f64 = 1100.0;
const HEIGHT: f64 = 650.0;
const MARGIN_LEFT: f64 = 90.0;
const MARGIN_RIGHT: f64 = 40.0;
const MARGIN_TOP: f64 = 50.0;
const MARGIN_BOTTOM: f64 = 80.0;

const Y_MIN: f64 = -20.0;
const Y_MAX: f64 = 20.0;

fn main() -> Result<(), Box<dyn Error>> {
	let year = env::args()
		.nth(1)
		.map(|value| value.parse::<i32>())
		.transpose()?
		.unwrap_or(DEFAULT_YEAR);

	let output_path = env::args().nth(2).map(PathBuf::from).unwrap_or_else(|| {
		PathBuf::from(format!("target/equation_of_time_{year}.svg"))
	});

	let samples = sample_equation_of_time(year)?;
	let svg = render_svg(year, &samples);

	if let Some(parent) = output_path.parent() {
		fs::create_dir_all(parent)?;
	}
	fs::write(&output_path, svg)?;

	let min_sample = samples
		.iter()
		.min_by(|left, right| left.minutes.total_cmp(&right.minutes))
		.expect("at least one sample");
	let max_sample = samples
		.iter()
		.max_by(|left, right| left.minutes.total_cmp(&right.minutes))
		.expect("at least one sample");

	println!("Wrote {}", output_path.display());
	println!("Min EOT: {:+.2} minutes on day {}", min_sample.minutes, min_sample.day_of_year);
	println!("Max EOT: {:+.2} minutes on day {}", max_sample.minutes, max_sample.day_of_year);

	Ok(())
}

#[derive(Clone, Copy)]
struct Sample {
	day_of_year: usize,
	minutes: f64,
}

fn sample_equation_of_time(year: i32) -> Result<Vec<Sample>, Box<dyn Error>> {
	let days_in_year = if is_leap_year(year) { 366 } else { 365 };
	let mut samples = Vec::with_capacity(days_in_year);

	let mut sun = make_sun_object();

	for day_of_year in 1..=days_in_year {
		let (month, day) = month_day_from_day_of_year(year, day_of_year as i32);
		let jd_utc = unsafe {
			novas::julian_date(
				year as i16,
				month as i16,
				day as i16,
				SAMPLE_HOUR_UTC,
			)
		};
		let jd_tt = jd_utc + DELTA_T_SECONDS / 86400.0;
		let jd_ut1 = jd_utc;
		let jd_high = jd_ut1.floor();
		let jd_low = jd_ut1 - jd_high;

		let mut sun_ra = 0.0;
		let mut sun_dec = 0.0;
		let mut sun_dis = 0.0;
		let app_status = unsafe {
			novas::app_planet(jd_tt, &mut sun, 1, &mut sun_ra, &mut sun_dec, &mut sun_dis)
		};
		if app_status != 0 {
			return Err(format!("app_planet failed on day {day_of_year}: {app_status}").into());
		}

		let mut gast = 0.0;
		let st_status = unsafe {
			novas::sidereal_time(jd_high, jd_low, DELTA_T_SECONDS, 1, 1, 0, &mut gast)
		};
		if st_status != 0 {
			return Err(format!("sidereal_time failed on day {day_of_year}: {st_status}").into());
		}

		let eot_minutes = wrapped_difference_hours(gast, sun_ra) * 60.0;
		samples.push(Sample {
			day_of_year,
			minutes: eot_minutes,
		});
	}

	Ok(samples)
}

fn make_sun_object() -> novas::object {
	let mut sun_name = [0 as c_char; 4];
	sun_name[0] = b'S' as c_char;
	sun_name[1] = b'u' as c_char;
	sun_name[2] = b'n' as c_char;

	let mut dummy_star = novas::cat_entry {
		starname: [0; 51],
		catalog: [0; 4],
		starnumber: 0,
		ra: 0.0,
		dec: 0.0,
		promora: 0.0,
		promodec: 0.0,
		parallax: 0.0,
		radialvelocity: 0.0,
	};

	let mut sun = novas::object {
		type_: 0,
		number: 10,
		name: [0; 51],
		star: dummy_star,
	};

	let status = unsafe {
		novas::make_object(0, 10, sun_name.as_mut_ptr(), &mut dummy_star, &mut sun)
	};
	assert_eq!(status, 0, "make_object failed for the Sun");
	sun
}

fn render_svg(year: i32, samples: &[Sample]) -> String {
	let plot_width = WIDTH - MARGIN_LEFT - MARGIN_RIGHT;
	let plot_height = HEIGHT - MARGIN_TOP - MARGIN_BOTTOM;

	let x_for_day = |day_of_year: usize| {
		let span = (samples.len().saturating_sub(1)).max(1) as f64;
		MARGIN_LEFT + ((day_of_year as f64 - 1.0) / span) * plot_width
	};

	let y_for_minutes = |minutes: f64| {
		let clamped = minutes.clamp(Y_MIN, Y_MAX);
		MARGIN_TOP + ((Y_MAX - clamped) / (Y_MAX - Y_MIN)) * plot_height
	};

	let mut svg = String::new();
	write!(
		svg,
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">
	<rect width="100%" height="100%" fill="#f8f6f2"/>
	<rect x="0" y="0" width="{WIDTH}" height="{HEIGHT}" fill="none" stroke="#d7d2c8"/>
	<text x="{title_x}" y="28" text-anchor="middle" font-family="serif" font-size="26" fill="#1f2933">Equation of Time over {year}</text>
	<text x="{subtitle_x}" y="48" text-anchor="middle" font-family="serif" font-size="13" fill="#5b6470">Computed from NOVAS apparent solar RA and Greenwich apparent sidereal time</text>
"##,
		title_x = WIDTH / 2.0,
		subtitle_x = WIDTH / 2.0
	)
	.expect("write SVG header");

	let chart_left = MARGIN_LEFT;
	let chart_top = MARGIN_TOP;
	let chart_right = WIDTH - MARGIN_RIGHT;
	let chart_bottom = HEIGHT - MARGIN_BOTTOM;

	write!(
		svg,
		r##"  <line x1="{chart_left}" y1="{chart_bottom}" x2="{chart_right}" y2="{chart_bottom}" stroke="#374151" stroke-width="1.5"/>
  <line x1="{chart_left}" y1="{chart_top}" x2="{chart_left}" y2="{chart_bottom}" stroke="#374151" stroke-width="1.5"/>
  <line x1="{chart_left}" y1="{zero_y}" x2="{chart_right}" y2="{zero_y}" stroke="#9ca3af" stroke-dasharray="6 5" stroke-width="1"/>
"##,
		zero_y = y_for_minutes(0.0)
	)
	.expect("write axes");

	for tick in (-20..=20).step_by(5) {
		let y = y_for_minutes(tick as f64);
		write!(
			svg,
			r##"  <line x1="{chart_left}" y1="{y}" x2="{chart_right}" y2="{y}" stroke="#e5e7eb" stroke-width="1"/>
  <text x="{label_x}" y="{label_y}" text-anchor="end" font-family="sans-serif" font-size="12" fill="#374151">{tick:+}m</text>
"##,
			label_x = chart_left - 8.0,
			label_y = y + 4.0
		)
		.expect("write y tick");
	}

	let mut points = String::new();
	for sample in samples {
		let x = x_for_day(sample.day_of_year);
		let y = y_for_minutes(sample.minutes);
		write!(points, "{x:.2},{y:.2} ").expect("write polyline point");
	}

	write!(
		svg,
		r##"  <polyline fill="none" stroke="#0f766e" stroke-width="2.5" points="{points}"/>
"##
	)
	.expect("write polyline");

	for (label, day_of_year) in month_ticks(year) {
		let x = x_for_day(day_of_year as usize);
		write!(
			svg,
			r##"  <line x1="{x}" y1="{chart_bottom}" x2="{x}" y2="{chart_bottom_plus}" stroke="#9ca3af" stroke-width="1"/>
  <text x="{x}" y="{month_label_y}" text-anchor="middle" font-family="sans-serif" font-size="11" fill="#374151">{label}</text>
"##,
			chart_bottom_plus = chart_bottom + 6.0,
			month_label_y = chart_bottom + 20.0
		)
		.expect("write month tick");
	}

	write!(
		svg,
		r##"  <text x="{center_x}" y="{x_label_y}" text-anchor="middle" font-family="sans-serif" font-size="13" fill="#374151">Day of year</text>
  <text x="20" y="{center_y}" text-anchor="middle" font-family="sans-serif" font-size="13" fill="#374151" transform="rotate(-90 20 {center_y})">Offset (minutes)</text>
</svg>
"##,
		center_x = WIDTH / 2.0,
		x_label_y = HEIGHT - 18.0,
		center_y = HEIGHT / 2.0
	)
	.expect("write footer");

	svg
}

fn month_ticks(year: i32) -> Vec<(&'static str, i32)> {
	let month_lengths = if is_leap_year(year) {
		[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
	} else {
		[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
	};

	let labels = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
	let mut ticks = Vec::with_capacity(12);
	let mut day = 1;
	for (index, label) in labels.iter().enumerate() {
		ticks.push((*label, day));
		day += month_lengths[index];
	}
	ticks
}

fn month_day_from_day_of_year(year: i32, day_of_year: i32) -> (i32, i32) {
	let month_lengths = if is_leap_year(year) {
		[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
	} else {
		[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
	};

	let mut remaining = day_of_year;
	for (index, month_length) in month_lengths.iter().enumerate() {
		if remaining <= *month_length {
			return ((index + 1) as i32, remaining);
		}
		remaining -= *month_length;
	}

	(12, 31)
}

fn is_leap_year(year: i32) -> bool {
	(year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn wrapped_difference_hours(left: f64, right: f64) -> f64 {
	let mut diff = left - right;
	if diff > 12.0 {
		diff -= 24.0;
	} else if diff < -12.0 {
		diff += 24.0;
	}
	diff
}
