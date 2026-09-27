"""CI-only SMTP adapter tests. No network or real credentials."""
import importlib.util
import io
import json
import base64
from pathlib import Path
import smtplib
import unittest
from unittest.mock import patch, MagicMock

spec = importlib.util.spec_from_file_location("briefing_mail", Path(__file__).with_name("send-briefing-mail.py"))
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)


class MailTests(unittest.TestCase):
    def run_mail(self, server, html_body=None, inline_images=None):
        payload = {"recipient":"reader@example.test", "subject":"晨报", "message_id":"<test@example.test>", "body":"论文内容"}
        if html_body is not None:
            payload['html_body'] = html_body
        if inline_images is not None:
            payload['inline_images'] = inline_images
        with patch.object(adapter.Path, "read_text", return_value="MAIL=sender@example.test\nPASSWD='fake-password'"), \
             patch.object(adapter.sys, "argv", ["mail", "/fake/credentials"]), \
             patch.object(adapter.sys, "stdin", io.StringIO(json.dumps(payload))), \
             patch.object(adapter.smtplib, "SMTP", return_value=server):
            return adapter.main()

    def test_tls_and_explicit_recipient_and_stable_message_id(self):
        server = MagicMock()
        self.assertEqual(self.run_mail(server), 0)
        server.starttls.assert_called_once()
        server.login.assert_called_once_with("sender@example.test", "fake-password")
        message = server.send_message.call_args.args[0]
        self.assertEqual(message["To"], "reader@example.test")
        self.assertEqual(message["Message-ID"], "<test@example.test>")
        self.assertNotIn("fake-password", message.as_string())
        server.close.assert_called_once()

    def test_html_is_preferred_with_complete_plain_text_fallback(self):
        server = MagicMock()
        html_body = '<html><body><h1>晨报</h1><p>论文内容</p></body></html>'
        self.assertEqual(self.run_mail(server, html_body), 0)
        message = server.send_message.call_args.args[0]
        self.assertEqual(message.get_content_type(), 'multipart/alternative')
        parts = list(message.iter_parts())
        self.assertEqual([part.get_content_type() for part in parts], ['text/plain', 'text/html'])
        self.assertEqual(parts[0].get_content().strip(), '论文内容')
        self.assertEqual(parts[1].get_content().strip(), html_body)
        self.assertEqual(message.get_body().get_content_type(), 'text/html')

    def test_disconnect_during_data_requires_manual_confirmation(self):
        server = MagicMock()
        server.send_message.side_effect = smtplib.SMTPServerDisconnected("connection lost")
        self.assertEqual(self.run_mail(server), 2)

    def test_inline_figure_is_related_to_html_not_a_remote_image(self):
        server = MagicMock()
        cid = 'figure-' + 'a' * 64 + '@paper-codex'
        png = b'\x89PNG\r\n\x1a\nfixture'
        image = {'cid': cid, 'mime': 'image/png', 'data_base64': base64.b64encode(png).decode()}
        self.assertEqual(self.run_mail(server, f'<img src="cid:{cid}">', [image, image]), 0)
        message = server.send_message.call_args.args[0]
        self.assertEqual(message.get_content_type(), 'multipart/alternative')
        related = list(message.iter_parts())[1]
        self.assertEqual(related.get_content_type(), 'multipart/related')
        html, attachment = list(related.iter_parts())
        self.assertEqual(html.get_content_type(), 'text/html')
        self.assertEqual(attachment['Content-ID'], f'<{cid}>')
        self.assertEqual(attachment.get_content_disposition(), 'inline')
        self.assertEqual(attachment.get_payload(decode=True), png)

    def test_rejects_bad_cid_and_image_types_before_connecting(self):
        server = MagicMock()
        with self.assertRaises(ValueError):
            self.run_mail(server, '<p>正文</p>', [{'cid':'bad\r\nheader', 'mime':'image/svg+xml', 'data_base64':''}])
        server.send_message.assert_not_called()

    def test_auth_failure_never_sends_data(self):
        server = MagicMock()
        server.login.side_effect = smtplib.SMTPAuthenticationError(535, b"rejected")
        with self.assertRaises(smtplib.SMTPAuthenticationError):
            self.run_mail(server)
        server.send_message.assert_not_called()

    def test_definitive_data_rejection_is_not_uncertain(self):
        server = MagicMock()
        server.send_message.side_effect = smtplib.SMTPDataError(550, b"rejected")
        self.assertEqual(self.run_mail(server), 4)

    def test_temporary_data_rejection_is_retryable(self):
        server = MagicMock()
        server.send_message.side_effect = smtplib.SMTPDataError(451, b"try later")
        self.assertEqual(self.run_mail(server), 1)

    def test_recipient_rejection_is_not_uncertain(self):
        for status, expected in ((550, 3), (450, 1)):
            with self.subTest(status=status):
                server = MagicMock()
                server.send_message.side_effect = smtplib.SMTPRecipientsRefused({'reader@example.test': (status, b'rejected')})
                self.assertEqual(self.run_mail(server), expected)


parser_spec = importlib.util.spec_from_file_location("briefing_html", Path(__file__).with_name("briefing-html-metadata.py"))
html_parser = importlib.util.module_from_spec(parser_spec)
parser_spec.loader.exec_module(html_parser)


class MetadataTests(unittest.TestCase):
    def test_affiliations_are_from_author_block_and_overview_wins_over_plot(self):
        result = html_parser.extract('''<div class="ltx_authors"><span>Alice</span><span class="ltx_role_affiliation">Example University &amp; Lab</span></div>
        <figure id="F1"><img src="paper/plot.png"><figcaption>Figure 1: Accuracy curve</figcaption></figure>
        <figure id="F2"><img src="paper/overview.png"><figcaption>Figure 2: Method overview</figcaption></figure>
        <footer>Unrelated Funder University</footer>''')
        self.assertIn('Example University & Lab', result['affiliation_evidence'])
        self.assertNotIn('Unrelated Funder', result['affiliation_evidence'])
        self.assertEqual(result['figure']['figure_id'], 'F2')
        self.assertEqual(result['figure']['kind'], 'overview')

    def test_skips_logo_tables_and_partial_multi_panel_figures(self):
        result = html_parser.extract('''<img src="logo.png"><figure class="ltx_table"><img src="table.png"><figcaption>Table 1</figcaption></figure>
        <figure><img src="a.png"><img src="b.png"><figcaption>Teaser</figcaption></figure>''')
        self.assertIsNone(result['figure'])
        self.assertEqual(result['affiliation_evidence'], '')

    def test_teaser_priority_and_caption_entities(self):
        result = html_parser.extract('''<figure><img src="first.png"><figcaption>First</figcaption></figure>
        <figure><img src="teaser.png"><figcaption>A &lt; B</figcaption></figure>''')
        self.assertEqual(result['figure']['kind'], 'teaser')
        self.assertEqual(result['figure']['caption'], 'A < B')


if __name__ == "__main__":
    unittest.main()
