"""Physics vocabulary for the HWR lexicon, from your own files: LaTeX, Markdown/text notes and the PDFs of
your references, so the decoder does not replace obscure physics words with common ones.

A general dictionary makes the decoder turn rare physics words into common ones, so the lexicon also gets the words
this user reads and writes. LaTeX math, commands and environments are stripped; PDFs go through pdftotext. A word is
kept when it occurs in at least MIN_DOCS files (drops extraction junk and one-off typos). Output: one word per line,
most widespread first. It stays local: it is derived from the user's files.

usage: physics_vocab.py OUT.txt DIR [DIR ...]"""

import collections
import concurrent.futures
import os
import re
import subprocess
import sys

MIN_DOCS = 3
TEXT_EXT = (".tex", ".md", ".txt")
# Mail (HTML bodies), logs and VCS/tool folders are not prose about physics.
SKIP_DIRS = {"mail", "log", "logs", ".git", "node_modules", ".obsidian", "__pycache__"}
MATH = re.compile(r"\$\$.*?\$\$|\$[^$]*\$|\\\[.*?\\\]|\\\(.*?\\\)", re.S)
ENV_MATH = re.compile(r"\\begin\{(equation|align|gather|eqnarray|multline)\*?\}.*?\\end\{\1\*?\}", re.S)
COMMAND = re.compile(r"\\[A-Za-z@]+\*?(\[[^\]]*\])?")
COMMENT = re.compile(r"(?<!\\)%.*")
WORD = re.compile(r"[A-Za-z][a-z]+(?:-[A-Za-z][a-z]+)*")  # lower-case after the first letter: no acronyms/variables


def pdf_text(path):
    try:
        return subprocess.run(["pdftotext", "-q", "-enc", "UTF-8", path, "-"], capture_output=True, timeout=120).stdout.decode("utf-8", "ignore")
    except subprocess.TimeoutExpired:
        return ""


def read(path):
    if path.endswith(".pdf"):
        return re.sub(r"-\n(?=[a-z])", "", pdf_text(path))  # rejoin words hyphenated at line ends
    with open(path, encoding="utf-8", errors="ignore") as f:
        return f.read()


def words(tex):
    tex = COMMENT.sub("", tex)
    tex = ENV_MATH.sub(" ", tex)
    tex = MATH.sub(" ", tex)
    tex = COMMAND.sub(" ", tex)
    return (w.lower() if w[1:].islower() else w for w in WORD.findall(tex) if len(w) >= 3)


DICT = "/usr/share/dict/words"
VOWELS = set("aeiouy")


def plausible(w, known):
    """Dictionary words always; others (names, jargon) only when they look like words: PDF extraction leaves short
    letter salad (`cihg`, `xpn`) in many files."""
    if w.lower() in known:
        return True
    letters = w.replace("-", "").lower()
    if len(letters) < 5 or not VOWELS & set(letters):
        return False
    run = 0
    for c in letters:
        run = 0 if c in VOWELS else run + 1
        if run >= 5:
            return False
    return sum(c in VOWELS for c in letters) / len(letters) >= 0.25


def main():
    out, dirs = sys.argv[1], sys.argv[2:]
    paths = []
    for d in dirs:
        for r, sub, ns in os.walk(d):
            sub[:] = [x for x in sub if x.lower() not in SKIP_DIRS]
            paths += [os.path.join(r, n) for n in ns if n.lower().endswith(TEXT_EXT + (".pdf",))]
    print(f"[physics-vocab] {len(paths)} files ({sum(p.lower().endswith('.pdf') for p in paths)} PDFs)", flush=True)
    docs = collections.Counter()
    with concurrent.futures.ThreadPoolExecutor(8) as ex:
        for k, ws in enumerate(ex.map(lambda p: set(words(read(p))), paths), 1):
            docs.update(ws)
            if k % 500 == 0:
                print(f"[physics-vocab] {k}/{len(paths)} read, {len(docs)} distinct words", flush=True)
    known = set()
    if os.path.exists(DICT):
        with open(DICT, encoding="utf-8", errors="ignore") as f:
            known = {w.strip().lower() for w in f}
    kept = [w for w, c in docs.most_common() if c >= MIN_DOCS and plausible(w, known)]
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(kept) + "\n")
    print(f"[physics-vocab] {len(docs)} distinct words, {len(kept)} in ≥ {MIN_DOCS} files → {out}")


if __name__ == "__main__":
    main()
