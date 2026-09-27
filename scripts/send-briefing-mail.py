"""Private SMTP adapter: credentials stay in a local dotenv file; body on stdin."""
import json
import ssl
import sys
import smtplib
from email.message import EmailMessage
from email.utils import formatdate
from pathlib import Path


def main():
    values = {}
    for line in Path(sys.argv[1]).read_text().splitlines():
        line = line.strip()
        if not line or line.startswith('#') or '=' not in line:
            continue
        key, value = line.removeprefix('export ').split('=', 1)
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        values[key.strip()] = value
    sender, password = values['MAIL'], values['PASSWD']
    payload = json.load(sys.stdin)
    message = EmailMessage()
    message['From'] = sender
    message['To'] = payload['recipient']
    message['Subject'] = payload['subject']
    message['Date'] = formatdate(localtime=False)
    message['Message-ID'] = payload['message_id']
    message.set_content(payload['body'])
    server = smtplib.SMTP(values.get('SMTP_HOST', 'smtp.qq.com'), int(values.get('SMTP_PORT', '587')), timeout=20)
    try:
        server.starttls(context=ssl.create_default_context())
        server.login(sender, password)
        try:
            server.send_message(message)
        except smtplib.SMTPDataError as error:
            # SMTPException inherits OSError. A definitive DATA rejection is
            # not an uncertain disconnect and must not trigger duplicate checks.
            return 4 if error.smtp_code >= 500 else 1
        except smtplib.SMTPResponseException as error:
            return 3 if error.smtp_code >= 500 else 1
        except smtplib.SMTPRecipientsRefused as error:
            return 3 if any(code >= 500 for code, _ in error.recipients.values()) else 1
        except (smtplib.SMTPServerDisconnected, OSError):
            # The server may have accepted DATA before the connection was lost.
            return 2
    finally:
        # Failure to QUIT after a successful DATA must not trigger duplicate mail.
        server.close()
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (smtplib.SMTPAuthenticationError, smtplib.SMTPNotSupportedError, KeyError, ValueError, FileNotFoundError):
        sys.exit(3)
    except smtplib.SMTPResponseException as error:
        sys.exit(3 if error.smtp_code >= 500 else 1)
    except Exception:
        # Never print exceptions that could include credentials or message content.
        sys.exit(1)
