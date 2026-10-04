"""Parse already-fetched arXiv HTML. No network, files, or third-party modules."""
import json
import re
import sys
from html.parser import HTMLParser
from urllib.parse import urlsplit


class Node:
    def __init__(self, tag='', attrs=()):
        self.tag = tag
        self.attrs = dict(attrs)
        self.children = []

    def nodes(self):
        yield self
        for child in self.children:
            if isinstance(child, Node):
                yield from child.nodes()

    def text(self):
        if self.tag in ('script', 'style'):
            return ''
        return ' '.join(child.text() if isinstance(child, Node) else child for child in self.children)


class Document(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.root = Node()
        self.stack = [self.root]

    def handle_starttag(self, tag, attrs):
        node = Node(tag, attrs)
        node.parent = self.stack[-1]
        self.stack[-1].children.append(node)
        if tag not in ('area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'param', 'source', 'track', 'wbr'):
            self.stack.append(node)

    def handle_endtag(self, tag):
        for index in range(len(self.stack) - 1, 0, -1):
            if self.stack[index].tag == tag:
                del self.stack[index:]
                break

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)
        self.handle_endtag(tag)

    def handle_data(self, data):
        self.stack[-1].children.append(data)


def extract(html):
    document = Document()
    document.feed(html)
    nodes = list(document.root.nodes())
    author_blocks = [node for node in nodes if 'ltx_authors' in node.attrs.get('class', '').split()]
    # Keep affiliation evidence with author superscripts; the editor must not
    # infer affiliations from author names or from the arXiv site footer.
    evidence = '\n'.join(re.sub(r'\s+', ' ', node.text()).strip() for node in author_blocks)[:12000]
    figures = []
    for position, node in enumerate(node for node in nodes if node.tag == 'figure'):
        if node.tag != 'figure' or 'ltx_table' in node.attrs.get('class', '').split():
            continue
        descendants = list(node.nodes())
        images = [item for item in descendants if (item.tag == 'img' and item.attrs.get('src'))
                  or (item.tag == 'object' and item.attrs.get('type') == 'image/svg+xml' and item.attrs.get('data'))]
        captions = [item for item in descendants if item.tag == 'figcaption']
        # Do not mistake one panel of a multi-image figure for the whole teaser.
        if len(images) != 1 or len(captions) != 1:
            continue
        caption = re.sub(r'\s+', ' ', captions[0].text()).strip()
        if not caption:
            continue
        src = images[0].attrs.get('src') or images[0].attrs['data']
        if not re.search(r'\.(png|jpe?g|svg)$', urlsplit(src).path, re.I):
            continue
        # Filenames are useful evidence, not the whole URL: an asset directory
        # named "teaser" must not turn every plot in that directory into one.
        filename = urlsplit(src).path.rsplit('/', 1)[-1].lower()
        hint = filename + ' ' + caption.lower()
        ancestor = node
        appendix = False
        while ancestor is not None:
            appendix |= bool(re.search(r'appendix|supplement', ancestor.attrs.get('class', ''), re.I))
            appendix |= bool(re.match(r'^A\d+(?:\.|$)', ancestor.attrs.get('id', '')))
            ancestor = getattr(ancestor, 'parent', None)
        if appendix or re.search(r'\b(logo|ablation|hyperparameter|sensitivity|success.rate|accuracy.curve)\b', hint):
            continue
        # Strip figure numbering; inspect the subject of the caption, not an
        # incidental mention of "our framework" halfway through an experiment.
        subject = re.sub(r'^(?:figure|fig\.?)\s*[\d.]+\s*[:.]?\s*', '', caption.lower())[:160]
        teaser = bool(re.search(r'(?:^|[\W_])(teaser|eyecatch)(?:[\W_]|$)', filename)
                      or re.match(r'(?:our )?teaser\b', subject))
        overview = bool(re.search(r'\b(overview|pipeline|architecture|framework)\b', subject)
                        or re.match(r'we (?:propose|present|introduce)\b', subject))
        demonstration = bool(re.match(r'(?:representative )?(?:demonstrations|capabilities)\b', subject))
        if not (teaser or overview or demonstration):
            continue  # Missing teaser is preferable to an unrelated results plot.
        if not teaser and re.match(r'(?:evaluation|experimental|real.robot setup|comparison|results)\b', subject):
            continue
        kind = 'teaser' if teaser else 'overview' if overview else 'demonstration'
        rank = (0 if teaser else 1 if overview else 2, position)
        figures.append((rank, {'image_ref': src, 'caption': caption[:900], 'figure_id': node.attrs.get('id', ''),
                              'kind': kind, 'format': 'svg' if filename.endswith('.svg') else 'raster',
                              'selection_reason': 'explicit_teaser' if teaser else 'caption_subject'}))
    figures.sort(key=lambda item: item[0])
    return {'affiliation_evidence': evidence, 'figure': figures[0][1] if figures else None,
            'figure_selection': 'selected' if figures else 'no_confident_complete_figure'}


if __name__ == '__main__':
    source = sys.stdin.read(2 * 1024 * 1024 + 1)
    if len(source) > 2 * 1024 * 1024:
        raise ValueError('HTML exceeds parsing budget')
    print(json.dumps(extract(source), ensure_ascii=False))
