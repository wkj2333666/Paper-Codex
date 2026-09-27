"""Private SMTP adapter: credentials stay in a local dotenv file; body on stdin."""
import json
import base64
import re
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
    if payload.get('html_body'):
        # multipart/alternative: full plain text first, HTML preferred by clients.
        message.add_alternative(payload['html_body'], subtype='html')
        html_part = message.get_payload()[-1]
        total = 0
        seen = set()
        for image in payload.get('inline_images', []):
            cid, mime = image['cid'], image['mime']
            if not re.fullmatch(r'figure-[a-f0-9]{64}@paper-codex', cid) or mime not in ('image/png', 'image/jpeg'):
                raise ValueError('Invalid inline image')
            if cid in seen:
                continue
            seen.add(cid)
            raw = base64.b64decode(image['data_base64'], validate=True)
            total += len(raw)
            if len(raw) > 1024 * 1024 or total > 5 * 1024 * 1024:
                raise ValueError('Inline image budget exceeded')
            expected = b'\x89PNG\r\n\x1a\n' if mime == 'image/png' else b'\xff\xd8\xff'
            if not raw.startswith(expected):
                raise ValueError('Inline image type mismatch')
            subtype = mime.split('/')[1]
            html_part.add_related(raw, maintype='image', subtype=subtype, cid=f'<{cid}>', disposition='inline', filename=f'{cid.split("@")[0]}.{subtype}')
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
