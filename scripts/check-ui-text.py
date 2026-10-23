#!/usr/bin/env python3
"""Fail when Chinese text is written into code instead of the message files (NFR-05, ARC-38).

Interface text lives in MyDeployMate-Tauri/src/i18n/ (frontend) or is sent as a message code
(Rust, `mdm_core::Message`). Comments, tests and generated bindings may contain Chinese.

Usage: scripts/check-ui-text.py [APP_DIR]   (default: MyDeployMate-Tauri)
"""
import pathlib
import re
import sys

CJK = re.compile(r'[　-〿㐀-䶿一-鿿＀-￯]')

# Frontend files that may hold Chinese outside comments.
FRONTEND_SKIP = (
    'src/i18n/',  # the message files themselves
    'src/ipc/bindings/',  # generated from Rust doc comments
    'src/views/dev/',  # developer-only component gallery
)


def strip_comments(text, line_comment='//'):
    """Blank out /* */, <!-- --> and line comments, keeping line numbers and string contents."""
    out = []
    i, n = 0, len(text)
    quote = None
    while i < n:
        c = text[i]
        if quote:
            out.append(c)
            if c == '\\' and i + 1 < n:
                out.append(text[i + 1])
                i += 2
                continue
            if c == quote or (c == '\n' and quote != '`'):
                quote = None
            i += 1
            continue
        if text.startswith('/*', i) or text.startswith('<!--', i):
            end = text.find('*/' if c == '/' else '-->', i)
            end = n if end < 0 else end + (2 if c == '/' else 3)
            out.append(re.sub(r'[^\n]', ' ', text[i:end]))
            i = end
            continue
        if text.startswith(line_comment, i):
            end = text.find('\n', i)
            end = n if end < 0 else end
            out.append(' ' * (end - i))
            i = end
            continue
        if c in '"\'`':
            quote = c
        out.append(c)
        i += 1
    return ''.join(out)


def rust_code(path):
    text = path.read_text(encoding='utf-8')
    # Unit tests sit in a trailing `#[cfg(test)]` module.
    cut = re.search(r'^#\[cfg\((all\()?test\b', text, re.M)
    if cut:
        text = text[: cut.start()]
    # Rust lifetimes ('a) would read as open quotes; strings only use double quotes.
    text = re.sub(r"'(?:[^'\\\n]|\\.)'", "' '", text)
    return strip_comments(text.replace("'", ' '))


def frontend_code(path):
    text = path.read_text(encoding='utf-8')
    if path.suffix == '.vue':
        text = re.sub(r'<style[\s\S]*?</style>', lambda m: re.sub(r'[^\n]', ' ', m.group(0)), text)
    return strip_comments(text)


def files(app):
    for path in sorted(app.glob('src/**/*')):
        rel = path.relative_to(app).as_posix()
        if path.suffix not in ('.ts', '.vue') or rel.endswith('.test.ts'):
            continue
        if not rel.startswith(FRONTEND_SKIP):
            yield path, frontend_code
    for root in sorted(app.glob('crates/*/src')) + [app / 'src-tauri' / 'src']:
        for path in sorted(root.rglob('*.rs')):
            # Tests, and sample workspaces whose names are user content.
            if not re.search(r'(^|_)tests?\.rs$|^fixtures\.rs$', path.name):
                yield path, rust_code


def main():
    app = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else 'MyDeployMate-Tauri')
    found = 0
    for path, code in files(app):
        for no, line in enumerate(code(path).splitlines(), 1):
            if CJK.search(line):
                found += 1
                print(f'{path}:{no}: {line.strip()}')
    if found:
        print(
            f'\n{found} line(s) with Chinese text outside the message files. Put frontend text in '
            'src/i18n/zh-CN.ts and send backend text as a Message code (see AGENTS.md).',
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
