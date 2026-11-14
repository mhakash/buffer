//! Daily one-liners for the sidebar. Rotates by day of year; edit freely.

use chrono::{Datelike, Local, Timelike};

const LINES: &[&str] = &[
    "Small steps, every day, compound into big things.",
    "Do the hard thing first. The rest of the day gets lighter.",
    "Clarity comes from doing, not from thinking about doing.",
    "Ship it, then make it better.",
    "One task at a time. Then the next one.",
    "Done is a feature.",
    "Protect your focus like it's your scarcest resource. It is.",
    "Momentum beats motivation.",
    "Make today's list shorter than yesterday's.",
    "The best time to start was earlier. The next best is now.",
    "Finish something today you'll be glad about tomorrow.",
    "Deep work, then deep rest.",
    "Simplicity is a decision you make again and again.",
    "If it takes two minutes, do it now.",
    "Progress, not perfection.",
    "Be the person who closes the loop.",
    "Write it down. Free your head for better things.",
    "Calm is a superpower.",
    "Your future self is watching. Make them proud.",
    "Great work is mostly showing up, again.",
];

pub fn today() -> &'static str {
    LINES[Local::now().ordinal0() as usize % LINES.len()]
}

pub fn greeting() -> &'static str {
    match Local::now().hour() {
        5..12 => "Good morning",
        12..17 => "Good afternoon",
        17..22 => "Good evening",
        _ => "Burning the midnight oil",
    }
}
