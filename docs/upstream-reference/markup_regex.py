import regex as re

ITALIC_ASTERISK = re.compile(
    r"""
    (?<![\\*])      # Can't start with \ or an extra *
    \*              # Start with *
    (?P<text>       # Text content group
        (?:         # either
            [^\s*\\]    # a single character that is not whitespace, _ or backslash
        |           # or
            [^\s*]      # Can't start text with whitespace or *
            .*?
            [^\\*]      # Can't end text with * or \
        )
    )
    \*              # End with *
    (?!\*)          # Can't end with extra *
    """,
    flags=re.VERBOSE
    )
ITALIC_UNDERSCORE = re.compile(
    r"""
    (?<![\\_\p{L}\p{N}])    # Can't be preceded by \, an extra _ or alphanumeric characters
    _                       # Start with _
    (?P<text>               # Text content group
        (?:                 # either
            [^\s_\\]            # a single character that is not whitespace, _ or backslash
        |                   # or
            [^\s_]              # Can't start text with whitespace or _
            .*?
            [^\\_]              # Can't end text with _ or \
        )
    )
    _                       # End with _
    (?![\p{L}\p{N}_])       # Can't follow up with alphanumeric character or _
    """,
    flags=re.VERBOSE
    )
BOLD = re.compile(
    r"""
    (?<!\\)         # Can't be preceded by \
    (\*\*|__)       # Delimiter start (** or __)
    (?P<text>       # Text content group
        (?:         # either
            [^\s_*\\]   # a single character that is not whitespace, _, * or backslash
        |           # or
            [^\s*]      # Can't start with whitespace
            .*?
            [^\\]       # Can't end text with \
        )
    )
    \1              # Delimiter (matches opening)
    """,
    flags=re.VERBOSE
    )
BOLD_ITALIC = re.compile(
    r"""
    (?<!\\)         # Can't be preceded by \
    ((\*\*|__)([*_])|([*_])(\*\*|__))
    (?P<text>
        (?:         # either
            [^\s_*\\]   # a single character that is not whitespace, _, * or backslash
        |           # or
            [^\s*]      # Can't start with whitespace
            .*?
            [^\\]       # Can't end text with \
        )
    )
    (?:\5\4|\3\2)
    """,
    flags=re.VERBOSE)
STRIKETHROUGH = re.compile(
    r"""
    (?<!\\)         # Can't be preceded by \
    ~~
    (?P<text>
        (?:         # either
            [^\s_*\\]   # a single character that is not whitespace, _, * or backslash
        |           # or
            [^\s*]      # Can't start with whitespace
            .*?
            [^\s\\]     # Can't end text with \ or whitespace
        )
    )
    ~~
    """,
    flags=re.VERBOSE)
CODE = re.compile(
    r"""
    (?<!`)
    (?P<ticks>`+)
    (?!`)
    (?P<text>.+?)    #
    (?<!`)
    (?P=ticks)
    (?!`)
    """,
    flags=re.VERBOSE)
LINK = re.compile(
    r"""
    \[(?P<text>.*?)\]           # [link text]
    \(                          # opening parenthesis
        (?P<url>.+?)            # URL
        (?:[ ]\"(?P<title>.+)\")? # optional title
    \)
    """,
    flags=re.VERBOSE)
LINK_ALT = re.compile(
    r"""
    <(?P<text>(?P<url>((https?|ftp):[^'">\s]+)))>
    """,
    flags=re.VERBOSE)
URL = re.compile(
    r"[(http(s)?):\/\/(www\.)?a-zA-Z0-9@:%._\+~#=]{2,256}\.[a-z]{2,6}\b([-a-zA-Z0-9@:%_\+.~#?&//=]*)", re.I)
IMAGE = re.compile(
    r"""
    !\[(?P<text>.*?)\]
    \(
        (?P<url>.+?)
        (?:[ ]\"(?P<title>.+)\")?
    \)
    """,
    flags=re.VERBOSE)
HORIZONTAL_RULE = re.compile(
    r"""
    (?:^|\n{2,})[ ]{0,3}             # Maximum 3 spaces at the beginning of the line.
    (?P<symbols>
        (-[ ]{0,2}){3,} | # 3 or more hyphens, with 2 spaces maximum between each hyphen.
        (_[ ]{0,2}){3,} | # Idem, but with underscores.
        (\*[ ]{0,2}){3,}  # Idem, but with asterisks.
    )
    [ \t]*                # Optional trailing spaces or tabs.
    (?:\n{2,}|$)          # either two or more newlines, or end of document 
    """,
    flags=re.VERBOSE)
LIST = re.compile(
    r"""
    (?:^|\n)                          # assert start of line or newline
    (?P<content>
        (?P<indent>(?:\t|[ ]{2})*)    # tab or 4 spaces, any number of times
        (?P<symbol>(?:[\-*+]))        # the bullet can be - * or +
        [ ](?!\[[xX ]\])              # don't match checklist ([ ]/ [x] / [X])
        (?:\t|[ ]{2})*                # tab or 4 spaces, any number of times
        (?P<text>
            .+
        )?                            # we don't match multiline text, but it's ok
    )
    """,
    flags=re.VERBOSE)
CHECKLIST = re.compile(
    r"""
    (?:^|\n)                          # assert start of line or newline
    (?P<content>
        (?P<indent>(?:\t|[ ]{2})*)    # tab or 2 spaces, any number of times
        (?P<symbol>(?:[\-*+]))        # the bullet can be - * or +
        [ ]\[(?P<check>(?:[xX ]))\][ ]# match checklist ([ ]/ [x] / [X])
        (?:\t|[ ]{4})*                # tab or 4 spaces, any number of times
        (?P<text>
            .+
        )?                            # we don't match multiline text, but it's ok
    )
    """,
    flags=re.VERBOSE)
ORDERED_LIST = re.compile(
    r"""
    (?:^|\n)                          # assert start of line or newline
    (?P<content>
        (?P<indent>(?>\t|[ ]{2})*)    # tab or 4 spaces, any number of times
        (?P<prefix>
            (?:
                (?P<number>\d+)       # either a digit, any number of times
            |
                [a-z]+                # or a letter, any number of times
            )
            (?P<delimiter>[.)])       # . or ) as delimiter
        )
        [ \t]*                        # at least a tab or space after the delimiter
        (?P<text>
            .+
        )?
    )
    """,
    flags=re.VERBOSE)
BLOCK_QUOTE = re.compile(
    r"""
    ^[ ]{0,3}(?:>[ ]?)+(?P<text>.+)
    """,
    flags=re.VERBOSE|re.MULTILINE)
HEADER = re.compile(
    r"""
    ^                       # start of line
    (?P<level>\#{1,6})[ ]   # between 1 and 6 "#" followed by a space
    (?P<text>[^\n]+)        # any text until a newline
    """,
    flags=re.VERBOSE|re.MULTILINE)
HEADER_UNDER = re.compile(
    r"""
    ^                       # start of line
    (?P<text>.+)[ \t]*      # match any text (not including trailing whitespace)
    \n
    [=-]+[ \t]*             # any number of = or -, followed by optional trailing whitespace
    (?:\n)                # end with newline
    """,
    flags=re.VERBOSE|re.MULTILINE)
CODE_BLOCK = re.compile(
    r"""
    ^[ ]{0,3}
    (?P<block>
        ([`~]{3})           # ``` or ~~~
        (?P<text>.+?)       # any text
        (?<![ ])[ ]{0,3}    # no more than three spaces before the closing 
        \2                  # ``` or ~~~ (same as before)
    )
    (?:\s+?$|$)             # end with newline or spaces + newline
    """,
    flags=re.VERBOSE|re.DOTALL|re.MULTILINE)
TABLE = re.compile(
    r"^[\-+]{5,}\n(?P<text>.+?)\n[\-+]{5,}\n", re.S)
MATH = re.compile(
    r"""
    ([$]{1,2})              # $ or $$
    (?P<text>
        [^`\\ ]{1,2}        # if we have one/two chars, they can't be `,\ or space
        |
        [^` ].+?[^`\\ ])    # otherwise, the string can't start with `,space, or end with '',\ or space
    \1                      # $ or $$ (same as before)
    """,
    flags=re.VERBOSE|re.DOTALL|re.MULTILINE)
FOOTNOTE_ID = re.compile(
    r"""
    (?P<text>[^\s]+)        # any text without spaces
    \[\^(?P<id>[^\s]+)\]    # [^id_without_spaces]
    """,
    flags=re.VERBOSE)
FOOTNOTE = re.compile(
    r"""
    [ ]{0,3}                        # trailing spaces
    \[\^(?P<id>[^\s]+)\]:           # [^id_without_spaces]
    [ ]{0,7}                        # trailing space
    (?P<text>
        (?:
            [^\n]+                  # either text without newlines
        |
            \n+(?=(?:\t|[ ]{4}))    # or a newline followed by a tab/ 4 spaces
        )+                          # any number of times
    )
    (?:\n+|$)                       # an extra newline or end of document
    """,
    flags=re.VERBOSE|re.MULTILINE)
FRONTMATTER = re.compile(
    r"^(?:---)\n(?P<text>.+?)\n(?:---|\.{3})", re.DOTALL
)
