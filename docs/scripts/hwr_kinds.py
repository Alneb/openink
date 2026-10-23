"""Text vs math labels. Notes mix prose and formulas; the text recogniser trains on prose only and math goes to
its own recogniser (a 2-D layout of sub/superscripts is not a left-to-right character sequence).

One labels file per source; the kind is derived here so the split has a single definition."""

import re

# Prose: letters, digits, spaces and sentence punctuation. Anything else (Greek, = + | < > ( ) ∂ Σ …) is math.
_PROSE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9'.,;:!?/\- ]*$")
# Inside prose, a letter glued to a digit (H0, t1, 2M) or a lower→upper flip inside a word (iHt, aB) is a formula.
_VARIABLE = re.compile(r"[a-z][A-Z]|[A-Za-z][0-9]|[0-9][A-Za-z]")
_NUMBER = re.compile(r"^[0-9][0-9./\-]*$")  # 360, 24-03-26


def is_math(label: str) -> bool:
    if _NUMBER.match(label):
        return False
    if not _PROSE.match(label) or _VARIABLE.search(label):
        return True
    return sum(c.isalpha() for c in label) < 2  # a lone letter is a variable


def kind(label: str) -> str:
    return "math" if is_math(label) else "text"
