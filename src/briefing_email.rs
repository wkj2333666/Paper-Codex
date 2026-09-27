//! Email-safe Markdown with inline styles and an unchanged plain-text alternative.
use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag, TagEnd};
use serde_json::Value;

fn figure<'a>(markdown: &str, source: &'a Value) -> Option<&'a Value> {
    let url = source["paper"]["source_url"].as_str()?;
    let figure = &source["presentation"]["figure"];
    let image_url = url::Url::parse(figure["image_url"].as_str()?).ok()?;
    let paper_html = url::Url::parse(source["presentation"]["source_url"].as_str()?).ok()?;
    // Only publisher figures selected from this paper's HTML, never model-
    // supplied remote images or arbitrary trackers, including in cached data.
    let allowed = |url: &url::Url| {
        url.scheme() == "https"
            && url.host_str() == Some("arxiv.org")
            && url.port_or_known_default() == Some(443)
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path().starts_with("/html/")
    };
    let paper_id = paper_html
        .path()
        .strip_prefix("/html/")?
        .trim_end_matches('/');
    let image_id = image_url.path().strip_prefix("/html/")?.split('/').next()?;
    // arXiv's unversioned HTML may reference images under the resolved vN
    // directory. Explicitly versioned sources must still match exactly.
    let same_paper = image_id == paper_id
        || (!paper_id.contains('v')
            && crate::research::canonical_arxiv_id(image_id)
                == crate::research::canonical_arxiv_id(paper_id));
    (markdown.contains(&format!("]({url})"))
        && allowed(&image_url)
        && allowed(&paper_html)
        && same_paper
        && [".png", ".jpg", ".jpeg"]
            .iter()
            .any(|suffix| image_url.path().to_ascii_lowercase().ends_with(suffix)))
    .then_some(figure)
}

pub(crate) fn render_with_sources(day: &str, markdown: &str, sources: &[Value]) -> String {
    let mut output = render(day, markdown);
    for source in sources {
        let Some(figure) = figure(markdown, source) else {
            continue;
        };
        let paper_url = source["paper"]["source_url"].as_str().unwrap_or_default();
        let needle = format!("href=\"{}\">", escape(paper_url));
        // Skip the brief lead-in's "原文" link. Place the figure only in the
        // actual paper introduction, after its title/identity paragraph.
        let insertion = output.match_indices(&needle).find_map(|(start, _)| {
            let label_start = start + needle.len();
            let label_end = label_start + output[label_start..].find("</a>")?;
            if output[label_start..label_end].chars().count() < 16 {
                return None;
            }
            Some(label_end + output[label_end..].find("</p>")? + 4)
        });
        let Some(insertion) = insertion else {
            continue;
        };
        // The email and preview use the same HTML image, with no MIME image
        // parts that some clients surface as downloadable attachments.
        let src = figure["image_url"].as_str().unwrap_or_default();
        let label = match figure["kind"].as_str() {
            Some("teaser") => "论文 teaser",
            Some("overview") => "论文总览图",
            _ => "论文图示（未标注为 teaser）",
        };
        let caption = figure["caption"].as_str().unwrap_or_default();
        let short_caption: String = caption.chars().take(160).collect();
        let short_caption = if caption.chars().count() > 160 {
            format!("{short_caption}…（完整图注见原文）")
        } else {
            short_caption
        };
        let source_url = source["presentation"]["source_url"]
            .as_str()
            .unwrap_or(paper_url);
        let block = format!("<div style=\"margin:18px 0 22px;padding:12px;background-color:#ffffff;border:1px solid #dfe8e2;border-radius:8px;\"><a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\"><img src=\"{}\" referrerpolicy=\"no-referrer\" alt=\"{}\" width=\"620\" style=\"display:block;max-width:100%;width:100%;height:auto;border:0;\"></a><p style=\"margin:10px 0 0;font-size:13px;line-height:1.65;color:#52665c;\">{} · 原图及图注来自论文：{}<br><a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\" style=\"color:#246b53;\">查看原图</a>（若未显示，请在邮箱中允许加载图片）</p></div>", escape(source_url), escape(src), escape(caption), label, escape(&short_caption), escape(src));
        output.insert_str(insertion, &block);
    }
    output
}

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
        // Model-authored images become links. Only verified publisher figures
        // are inserted as images by render_with_sources.
        Event::End(TagEnd::Image) => Event::End(TagEnd::Link),
        other => other,
    });
    let mut body = String::new();
    html::push_html(&mut body, events);
    let mut cards = String::new();
    let mut card_open = false;
    for line in body.lines() {
        if (line.starts_with("<h2>") || line.starts_with("<h3>")) && card_open {
            cards.push_str("</div>\n");
            card_open = false;
        }
        if line.starts_with("<h3>") {
            cards.push_str("<div style=\"margin:20px 0;padding:18px;background-color:#f8faf7;border:1px solid #dfe8e2;border-radius:10px;\">\n");
            card_open = true;
            cards.push_str(&line.replacen(
                "<h3>",
                "<h3 style=\"font-size:20px;line-height:1.5;margin:0 0 14px;color:#193e31;\">",
                1,
            ));
        } else {
            cards.push_str(line);
        }
        cards.push('\n');
    }
    if card_open {
        cards.push_str("</div>\n");
    }
    body = cards;
    // Paper identity stays visible but secondary to the title. These prefixes
    // can only come from parsed Markdown: raw source HTML was escaped above.
    for label in ["作者：", "机构：", "资料："] {
        body = body.replace(
            &format!("<p><strong>{label}</strong>"),
            &format!("<p style=\"margin:8px 0 14px;font-size:14px;line-height:1.75;color:#52665c;\"><strong>{label}</strong>"),
        );
    }
    // Replacements only affect parser-generated tags. User HTML is escaped above.
    for (tag, styled) in [
        ("<h1>", "<h1 style=\"font-size:26px;line-height:1.4;margin:24px 0 16px;color:#172b26;\">"),
        ("<h2>", "<h2 style=\"font-size:21px;line-height:1.45;margin:30px 0 14px;padding:12px 14px;background-color:#edf5f1;border-left:4px solid #32755c;color:#193e31;\">"),
        ("<h3>", "<h3 style=\"font-size:21px;line-height:1.5;margin:34px 0 12px;padding:18px 0 0;border-top:2px solid #dfe8e2;color:#193e31;\">"),
        ("<h4>", "<h4 style=\"font-size:16px;line-height:1.5;margin:20px 0 8px;color:#193e31;\">"),
        ("<p>", "<p style=\"margin:10px 0 16px;line-height:1.8;\">"),
        ("<ul>", "<ul style=\"margin:10px 0 18px;padding-left:24px;\">"),
        ("<ol>", "<ol style=\"margin:10px 0 18px;padding-left:24px;\">"),
        ("<li>", "<li style=\"margin:7px 0;line-height:1.75;\">"),
        ("<a href=", "<a target=\"_blank\" rel=\"noopener noreferrer\" style=\"color:#246b53;text-decoration:underline;overflow-wrap:anywhere;\" href="),
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
<div style="font-family:Arial,sans-serif;font-size:14px;color:#d0e4d7;">{day} · 看懂新工作，再决定读什么</div></div>
<div style="padding:14px 22px;background-color:#edf5f1;font-family:'PingFang SC','Microsoft YaHei',Arial,sans-serif;font-size:14px;line-height:1.7;color:#315844;">先看今日导读，再读完整介绍：谁做了什么、如何实现、实验说明了什么。阅读判断放在事实之后。</div>
<div style="padding:8px 22px 24px;font-family:'PingFang SC','Microsoft YaHei',Arial,sans-serif;font-size:16px;line-height:1.8;overflow-wrap:anywhere;">{body}</div>
<div style="padding:18px 22px;background-color:#f6f8f5;border-top:1px solid #dfe8e2;font-family:'PingFang SC','Microsoft YaHei',Arial,sans-serif;font-size:12px;line-height:1.7;color:#637469;">由 Paper Codex 根据你的关注方向整理。请以论文原文为准。<br>全文与历史晨报保存在左侧「论文晨报」栏目；发送时间和关注方向可在栏目内的「设置」调整。</div>
</div></td></tr></table></body></html>"#,
        day = escape(day),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    #[test]
    fn email_preserves_paper_identity_and_styles_metadata_without_raw_markdown() {
        let output = render("2026-09-27", "## 今日导读\n\n### Example：具体的工作名称\n\n[Example: The Complete Original Title](https://example.test/paper)\n\n**作者：** Alice, Bob\n\n**具体怎么做：** 编码观测，再预测动作。\n\n**资料：** 全文节选\n");
        assert!(output.contains("Example: The Complete Original Title"));
        assert!(output.contains("<strong>作者：</strong> Alice, Bob"));
        assert!(output.contains("color:#52665c"));
        assert!(output.contains("<strong>具体怎么做：</strong>"));
        assert!(output.contains("target=\"_blank\" rel=\"noopener noreferrer\""));
        assert!(!output.contains("**作者"));
        assert!(!output.contains("## 今日导读"));
    }

    #[test]
    fn separates_paper_cards_and_uses_html_images_without_attachments() {
        let markdown = "## 今日导读\n\n[原文](https://arxiv.org/abs/1234.56789)\n\n## 顺手扫一眼\n\n### Example：工作一\n\n[Example: A Complete Paper Title](https://arxiv.org/abs/1234.56789)\n\n**作者：** Alice\n\n**机构：** Example University\n\n**做了什么：** 方法介绍。\n\n### Second：工作二\n\n另外一篇。\n\n## 阅读建议\n\n结束。";
        let source = json!({"paper":{"source_url":"https://arxiv.org/abs/1234.56789"}, "presentation":{"source_url":"https://arxiv.org/html/1234.56789v1","figure":{"image_url":"https://arxiv.org/html/1234.56789v1/overview.png","caption":"Overview <unsafe>","kind":"overview"}}});
        let sources = [source];
        let mail = render_with_sources("2026-09-27", markdown, &sources);
        assert!(mail.contains("src=\"https://arxiv.org/html/1234.56789v1/overview.png\""));
        assert!(!mail.contains("cid:"));
        assert!(!mail.contains("data:image/png"));
        assert!(mail.contains("Overview &lt;unsafe&gt;"));
        assert_eq!(mail.matches("<img ").count(), 1);
        assert_eq!(mail.matches("background-color:#f8faf7").count(), 2);
        assert!(
            mail.find("<img ").unwrap() > mail.find("Example: A Complete Paper Title").unwrap()
        );
        assert!(mail.contains("查看原图"));
        let mut unversioned_source = sources[0].clone();
        unversioned_source["presentation"]["source_url"] =
            json!("https://arxiv.org/html/1234.56789");
        assert!(
            render_with_sources("2026-09-27", markdown, &[unversioned_source]).contains("<img ")
        );
        assert!(!render_with_sources("2026-09-27", "没有选入的论文", &sources).contains("<img "));
        for image_url in [
            "https://tracker.test/figure.png",
            "https://arxiv.org/html/9999.99999v1/figure.png",
            "https://arxiv.org/html/1234.56789v2/figure.png",
            "https://arxiv.org/html/1234.56789v1/figure.png?tracking=1",
            "javascript:alert(1)",
        ] {
            let mut unsafe_source = sources[0].clone();
            unsafe_source["presentation"]["figure"]["image_url"] = json!(image_url);
            assert!(
                !render_with_sources("2026-09-27", markdown, &[unsafe_source]).contains("<img ")
            );
        }
    }
}
