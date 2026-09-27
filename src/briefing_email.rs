//! Email-safe Markdown with inline styles and an unchanged plain-text alternative.
use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag, TagEnd};

fn safe_destination(destination: CowStr<'_>) -> CowStr<'_> {
    match url::Url::parse(&destination) {
        Ok(url)
            if matches!(url.scheme(), "https" | "http")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none() =>
        {
            destination
        }
        _ => "#unavailable-link".into(),
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(crate) fn render(day: &str, markdown: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let events = Parser::new_ext(markdown, options).map(|event| match event {
        // Preserve raw HTML as visible text, not executable email markup.
        Event::Html(value) | Event::InlineHtml(value) => Event::Text(value),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        })
        | Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: safe_destination(dest_url),
            title,
            id,
        }),
        // Images become explicit links; opening the email never fetches them.
        Event::End(TagEnd::Image) => Event::End(TagEnd::Link),
        other => other,
    });
    let mut body = String::new();
    html::push_html(&mut body, events);
    // Replacements only affect parser-generated tags. User HTML is escaped above.
    for (tag, styled) in [
        ("<h1>", "<h1 style=\"font-size:26px;line-height:1.4;margin:24px 0 16px;color:#172b26;\">"),
        ("<h2>", "<h2 style=\"font-size:21px;line-height:1.45;margin:30px 0 14px;padding:12px 14px;background-color:#edf5f1;border-left:4px solid #32755c;color:#193e31;\">"),
        ("<h3>", "<h3 style=\"font-size:18px;line-height:1.5;margin:26px 0 10px;padding-top:18px;border-top:1px solid #dfe8e2;color:#193e31;\">"),
        ("<h4>", "<h4 style=\"font-size:16px;line-height:1.5;margin:20px 0 8px;color:#193e31;\">"),
        ("<p>", "<p style=\"margin:10px 0 16px;line-height:1.8;\">"),
        ("<ul>", "<ul style=\"margin:10px 0 18px;padding-left:24px;\">"),
        ("<ol>", "<ol style=\"margin:10px 0 18px;padding-left:24px;\">"),
        ("<li>", "<li style=\"margin:7px 0;line-height:1.75;\">"),
        ("<a href=", "<a style=\"color:#246b53;text-decoration:underline;overflow-wrap:anywhere;\" href="),
        ("<blockquote>", "<blockquote style=\"margin:16px 0;padding:4px 16px;border-left:3px solid #b2c9bb;background-color:#f5f8f5;color:#465d52;\">"),
        ("<pre>", "<pre style=\"padding:14px;background-color:#f1f4f2;border:1px solid #dfe8e2;border-radius:6px;white-space:pre-wrap;overflow-wrap:anywhere;font-size:13px;line-height:1.6;\">"),
        ("<code>", "<code style=\"font-family:Consolas,monospace;font-size:0.93em;\">"),
        ("<table>", "<table cellpadding=\"8\" cellspacing=\"0\" border=\"1\" style=\"width:100%;border-collapse:collapse;border-color:#dfe8e2;font-size:14px;table-layout:fixed;overflow-wrap:anywhere;\">"),
        ("<th>", "<th style=\"background-color:#edf5f1;color:#193e31;text-align:left;\">"),
        ("<hr />", "<hr style=\"border:0;border-top:1px solid #dfe8e2;margin:26px 0;\" />"),
    ] {
        body = body.replace(tag, styled);
    }
    format!(
        r#"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{day} · 具身论文晨报</title></head>
<body style="margin:0;padding:0;background-color:#f2f5f1;color:#263b31;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background-color:#f2f5f1;"><tr><td align="center" style="padding:20px 10px;">
<div style="max-width:720px;margin:0 auto;text-align:left;background-color:#ffffff;border:1px solid #dfe8e2;border-radius:12px;overflow:hidden;">
<div style="padding:26px 22px;background-color:#183e31;color:#ffffff;">
<div style="font-family:Arial,sans-serif;font-size:11px;letter-spacing:2px;color:#b5d9c5;">PAPER CODEX · DAILY BRIEFING</div>
<div style="font-family:'PingFang SC','Microsoft YaHei',Arial,sans-serif;font-size:28px;font-weight:bold;line-height:1.5;margin:8px 0;">具身论文晨报</div>
<div style="font-family:Arial,sans-serif;font-size:14px;color:#d0e4d7;">{day} · 先看要点，再决定今天读什么</div></div>
<div style="padding:14px 22px;background-color:#edf5f1;font-family:'PingFang SC','Microsoft YaHei',Arial,sans-serif;font-size:14px;line-height:1.7;color:#315844;">先读「30 秒速览」，再按需展开重点论文。观点、依据和关键限制放在一起。</div>
<div style="padding:8px 22px 24px;font-family:'PingFang SC','Microsoft YaHei',Arial,sans-serif;font-size:16px;line-height:1.8;overflow-wrap:anywhere;">{body}</div>
<div style="padding:18px 22px;background-color:#f6f8f5;border-top:1px solid #dfe8e2;font-family:'PingFang SC','Microsoft YaHei',Arial,sans-serif;font-size:12px;line-height:1.7;color:#637469;">由 Paper Codex 根据你的关注方向整理。请以论文原文为准。<br>全文与历史晨报保存在左侧「论文晨报」栏目；发送时间和关注方向可在栏目内的「设置」调整。</div>
</div></td></tr></table></body></html>"#,
        day = escape(day),
    )
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn renders_headings_lists_tables_and_links_without_losing_content() {
        let output = render("2026-09-27", "## 今日概览\n\n**重点**\n\n- 第一篇\n- 第二篇\n\n[论文原文](https://arxiv.org/abs/2504.16054)\n\n|方法|结果|\n|---|---|\n|VLA|提升|\n\n```text\nA < B\n```\n");
        for expected in [
            "今日概览",
            "<strong>重点</strong>",
            "第一篇",
            "第二篇",
            "href=\"https://arxiv.org/abs/2504.16054\"",
            "<table cellpadding",
            "提升",
            "A &lt; B",
            "white-space:pre-wrap",
            "max-width:720px",
        ] {
            assert!(output.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn does_not_execute_raw_html_or_load_remote_images() {
        let output = render("<bad>", "<script>alert(1)</script>\n\n<img src=\"https://tracker.test/pixel\" onerror=\"alert(1)\">\n\n![图示](https://example.test/figure.png)\n\n[危险](javascript:alert%281%29)\n\n[附件](data:text/html,hello)");
        assert!(!output.contains("<script>"));
        assert!(!output.contains("<img"));
        assert!(!output.contains("href=\"javascript:"));
        assert!(!output.contains("href=\"data:"));
        assert!(output.contains("&lt;bad&gt;"));
        assert!(output.contains("href=\"https://example.test/figure.png\""));
        assert!(output.contains("图示"));
    }
}
