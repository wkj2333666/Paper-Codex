"""Parse already-fetched arXiv HTML. No network, files, or third-party modules."""
import json
import re
import sys
from html.parser import HTMLParser


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
    for node in nodes:
        if node.tag != 'figure' or 'ltx_table' in node.attrs.get('class', '').split():
            continue
        descendants = list(node.nodes())
        images = [item for item in descendants if item.tag == 'img' and item.attrs.get('src')]
        captions = [item for item in descendants if item.tag == 'figcaption']
        # Do not mistake one panel of a multi-image figure for the whole teaser.
        if len(images) != 1 or len(captions) != 1:
            continue
        caption = re.sub(r'\s+', ' ', captions[0].text()).strip()
        if not caption:
            continue
        src = images[0].attrs['src']
        if not re.search(r'\.(png|jpe?g)(?:\?|$)', src, re.I):
            continue
        hint = (src + ' ' + caption).lower()
        rank = 0 if 'teaser' in hint else 1 if re.search(r'overview|pipeline|framework', hint) else 2
        figures.append((rank, {'image_ref': src, 'caption': caption[:900], 'figure_id': node.attrs.get('id', ''), 'kind': 'teaser' if rank == 0 else 'overview' if rank == 1 else 'first_figure'}))
    figures.sort(key=lambda item: item[0])
    return {'affiliation_evidence': evidence, 'figure': figures[0][1] if figures else None}


if __name__ == '__main__':
    source = sys.stdin.read(2 * 1024 * 1024 + 1)
    if len(source) > 2 * 1024 * 1024:
        raise ValueError('HTML exceeds parsing budget')
    print(json.dumps(extract(source), ensure_ascii=False))
