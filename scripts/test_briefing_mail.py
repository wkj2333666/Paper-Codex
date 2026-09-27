"""CI-only SMTP adapter tests. No network or real credentials."""
import importlib.util
import io
from pathlib import Path
import smtplib
import unittest
from unittest.mock import patch, MagicMock

spec = importlib.util.spec_from_file_location("briefing_mail", Path(__file__).with_name("send-briefing-mail.py"))
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)


class MailTests(unittest.TestCase):
    def run_mail(self, server):
        with patch.object(adapter.Path, "read_text", return_value="MAIL=sender@example.test\nPASSWD='fake-password'"), \
             patch.object(adapter.sys, "argv", ["mail", "/fake/credentials"]), \
             patch.object(adapter.sys, "stdin", io.StringIO('{"recipient":"reader@example.test","subject":"晨报","message_id":"<test@example.test>","body":"论文内容"}')), \
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

    def test_disconnect_during_data_requires_manual_confirmation(self):
        server = MagicMock()
        server.send_message.side_effect = smtplib.SMTPServerDisconnected("connection lost")
        self.assertEqual(self.run_mail(server), 2)

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


if __name__ == "__main__":
    unittest.main()
