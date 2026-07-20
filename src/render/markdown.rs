//! Markdown backend: a plain-text-friendly rendering of the shared model.
//!
//! Unlike the HTML/LaTeX/EPUB backends this is written by hand rather than via a
//! template — Markdown has little structure to escape and the line-by-line control
//! keeps the output diff-friendly and readable in any editor.

use std::fmt::Write as _;

use crate::model::{BookView, MsgView};

/// Who a message is attributed to. Falls back to the book title for 1:1 chats (where
/// received messages carry no explicit sender) and a generic label otherwise.
fn speaker(m: &MsgView, book_title: &str, is_group: bool) -> String {
    if m.from_me {
        "You".to_string()
    } else if let Some(s) = &m.sender {
        s.clone()
    } else if !is_group {
        book_title.to_string()
    } else {
        "Them".to_string()
    }
}

/// Render the book to a Markdown document.
pub fn render(book: &BookView) -> String {
    let mut out = String::new();

    writeln!(out, "# {}\n", book.title).ok();
    if let Some(author) = &book.author {
        writeln!(out, "*Assembled by {author}*\n").ok();
    }
    if let Some(dedication) = &book.dedication {
        writeln!(out, "> {dedication}\n").ok();
    }

    if let Some(stats) = &book.stats {
        writeln!(out, "## By the numbers\n").ok();
        writeln!(
            out,
            "- **Messages:** {} ({} from you, {} received)",
            stats.total, stats.from_me, stats.from_others
        )
        .ok();
        writeln!(
            out,
            "- **Span:** {} – {} ({} day{})",
            stats.first_date,
            stats.last_date,
            stats.days,
            if stats.days == 1 { "" } else { "s" }
        )
        .ok();
        writeln!(out, "- **Words sent:** {}", stats.words).ok();
        writeln!(out, "- **Photos:** {}", stats.photos).ok();
        if let Some(day) = &stats.busiest_day {
            writeln!(out, "- **Busiest day:** {day}").ok();
        }
        if let Some(hour) = &stats.busiest_hour {
            writeln!(out, "- **Busiest hour:** {hour}").ok();
        }
        if stats.longest_streak > 1 {
            writeln!(out, "- **Longest streak:** {} days", stats.longest_streak).ok();
        }
        if !stats.top_emoji.is_empty() {
            let emoji: Vec<String> = stats
                .top_emoji
                .iter()
                .map(|e| format!("{}×{}", e.emoji, e.count))
                .collect();
            writeln!(out, "- **Top emoji:** {}", emoji.join(" ")).ok();
        }
        if book.is_group && !stats.per_sender.is_empty() {
            let who: Vec<String> = stats
                .per_sender
                .iter()
                .map(|s| format!("{} ({})", s.name, s.count))
                .collect();
            writeln!(out, "- **Most active:** {}", who.join(", ")).ok();
        }
        out.push('\n');
    }

    for chapter in &book.chapters {
        writeln!(out, "## {}\n", chapter.title).ok();
        for day in &chapter.days {
            writeln!(out, "### {}\n", day.heading).ok();
            for m in &day.messages {
                if let Some(system) = &m.system {
                    writeln!(out, "_{system}_\n").ok();
                    continue;
                }
                if let Some(gap) = &m.gap_before {
                    writeln!(out, "_… {gap}_\n").ok();
                }

                // Attribution line: **Who** · time · edited · effect.
                let mut head = format!(
                    "**{}** · {}",
                    speaker(m, &book.title, book.is_group),
                    m.time
                );
                if m.edited {
                    head.push_str(" · edited");
                }
                if let Some(effect) = &m.effect {
                    write!(head, " · {effect}").ok();
                }
                writeln!(out, "{head}\n").ok();

                if let Some(reply) = &m.reply_to {
                    writeln!(out, "> ↩︎ {reply}\n").ok();
                } else if m.reply {
                    writeln!(out, "> ↩︎ Reply\n").ok();
                }
                if let Some(app) = &m.app {
                    writeln!(out, "**{app}**\n").ok();
                }
                for a in &m.attachments {
                    match (&a.src, a.kind.as_str()) {
                        (Some(src), "image" | "video") => {
                            writeln!(out, "![{}]({})", a.label, src).ok();
                            if let Some(caption) = &a.caption {
                                writeln!(out, "*{caption}*").ok();
                            }
                        }
                        _ => {
                            let caption = a
                                .caption
                                .as_ref()
                                .map(|c| format!(" — {c}"))
                                .unwrap_or_default();
                            writeln!(out, "📎 {}{}", a.label, caption).ok();
                        }
                    }
                }
                if let Some(text) = &m.text {
                    writeln!(out, "{text}").ok();
                }
                if !m.tapbacks.is_empty() {
                    writeln!(out, "\n_{}_", m.tapbacks.join("  ")).ok();
                }
                out.push('\n');
            }
        }
    }

    if !book.gallery.is_empty() {
        writeln!(out, "## Photos\n").ok();
        for src in &book.gallery {
            writeln!(out, "![photo]({src})").ok();
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChapterView, DayView};

    fn sample() -> BookView {
        let mut them = MsgView {
            time: "3:42 PM".to_string(),
            text: Some("hi there".to_string()),
            sender: Some("Naomi".to_string()),
            service: "imessage".to_string(),
            ..Default::default()
        };
        them.tapbacks = vec!["❤️ Loved".to_string()];
        BookView {
            title: "Chat".to_string(),
            is_group: true,
            chapters: vec![ChapterView {
                title: "November 2020".to_string(),
                id: "ch-2020-11".to_string(),
                days: vec![DayView {
                    heading: "Monday, November 2".to_string(),
                    messages: vec![them],
                }],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn renders_headings_and_message() {
        let md = render(&sample());
        assert!(md.contains("# Chat"));
        assert!(md.contains("## November 2020"));
        assert!(md.contains("### Monday, November 2"));
        assert!(md.contains("**Naomi** · 3:42 PM"));
        assert!(md.contains("hi there"));
        assert!(md.contains("❤️ Loved"));
    }
}
