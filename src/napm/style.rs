use indicatif::ProgressStyle;
use std::sync::OnceLock;

use crate::napm::Napm;

static PROGRESS_BAR_STYLE: OnceLock<ProgressStyle> = OnceLock::new();
static PROGRESS_BAR_STYLE_FAILED: OnceLock<ProgressStyle> = OnceLock::new();

impl Napm {
    pub fn progress_bar_style(failed: bool) -> &'static ProgressStyle {
        let progress_chars = "=> ";

        if failed {
            PROGRESS_BAR_STYLE_FAILED.get_or_init(|| {
                ProgressStyle::with_template("[{elapsed:>3}] [{bar:40.red/blue}] [FAILED] {msg}")
                    .unwrap()
                    .progress_chars(progress_chars)
            })
        } else {
            PROGRESS_BAR_STYLE.get_or_init(|| {
                ProgressStyle::with_template(
                    "[{elapsed:>3}] [{bar:40.cyan/blue}] {percent:>3}% {msg}",
                )
                .unwrap()
                .progress_chars(progress_chars)
            })
        }
    }
}
