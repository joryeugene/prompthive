"""Small VT decoder for the cursor/erase sequences emitted by Crossterm.

This retains actual cell text for CLI layout review; it does not model terminal
fonts or colors. Unknown CSI operations are retained as a QA limitation.
"""

import codecs
import unicodedata


class VtScreen:
    def __init__(self, columns, rows):
        self.columns, self.rows = columns, rows
        self.cells = [[" "] * columns for _ in range(rows)]
        self.x = self.y = 0
        self.saved_cursor = (0, 0)
        self.alternate = False
        self.last_alternate = None
        self.normal = None
        self.unsupported = []
        self.pending = ""
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")

    def resize(self, columns, rows):
        self.cells = [
            (self.cells[y][:columns] if y < self.rows else []) + [" "] * max(0, columns - (min(self.columns, columns) if y < self.rows else 0))
            for y in range(rows)
        ]
        self.columns, self.rows = columns, rows
        self.x, self.y = min(self.x, columns - 1), min(self.y, rows - 1)

    def text(self):
        return "\n".join("".join(row).rstrip() for row in self.cells)

    def _clear(self):
        self.cells = [[" "] * self.columns for _ in range(self.rows)]

    def _linefeed(self):
        self.y += 1
        if self.y >= self.rows:
            self.cells.pop(0)
            self.cells.append([" "] * self.columns)
            self.y = self.rows - 1

    def _character(self, character):
        if character == "\r":
            self.x = 0
        elif character == "\n":
            self._linefeed()
        elif character == "\b":
            self.x = max(0, self.x - 1)
        elif character == "\t":
            self.x = min(self.columns - 1, (self.x // 8 + 1) * 8)
        elif character >= " ":
            if unicodedata.combining(character) or unicodedata.category(character) == "Cf":
                self.cells[self.y][max(0, self.x - 1)] += character
                return
            width = 2 if unicodedata.east_asian_width(character) in {"W", "F"} else 1
            if self.x + width > self.columns:
                self.x = 0
                self._linefeed()
            self.cells[self.y][self.x] = character
            if width == 2:
                self.cells[self.y][self.x + 1] = ""
            self.x += width

    def _csi(self, arguments, operation):
        private = arguments.startswith("?")
        values = [int(value or 0) for value in arguments.lstrip("?").split(";")]
        first = values[0] or 1
        if private and operation in {"h", "l"}:
            if 1049 in values:
                if operation == "h":
                    self.normal = (self.cells, self.x, self.y)
                    self._clear()
                    self.x = self.y = 0
                    self.alternate = True
                else:
                    self.last_alternate = self.text()
                    if self.normal is not None:
                        self.cells, self.x, self.y = self.normal
                    self.alternate = False
            elif any(value not in {25, 2004} for value in values):
                self.unsupported.append(f"CSI {arguments}{operation}")
        elif operation in {"H", "f"}:
            self.y = max(0, min(self.rows - 1, first - 1))
            self.x = max(0, min(self.columns - 1, (values[1] or 1) - 1 if len(values) > 1 else 0))
        elif operation in {"A", "B", "C", "D"}:
            self.y = max(0, min(self.rows - 1, self.y + (first if operation == "B" else -first if operation == "A" else 0)))
            self.x = max(0, min(self.columns - 1, self.x + (first if operation == "C" else -first if operation == "D" else 0)))
        elif operation == "G":
            self.x = min(self.columns - 1, first - 1)
        elif operation == "d":
            self.y = min(self.rows - 1, first - 1)
        elif operation == "J":
            if values[0] in {2, 3}:
                self._clear()
            elif values[0] == 0:
                self.cells[self.y][min(self.x, self.columns):] = [" "] * max(0, self.columns - self.x)
                for y in range(self.y + 1, self.rows):
                    self.cells[y] = [" "] * self.columns
            elif values[0] == 1:
                for y in range(self.y):
                    self.cells[y] = [" "] * self.columns
                self.cells[self.y][:self.x + 1] = [" "] * (self.x + 1)
        elif operation == "K":
            start = 0 if values[0] in {1, 2} else min(self.x, self.columns)
            end = self.x + 1 if values[0] == 1 else self.columns
            self.cells[self.y][start:end] = [" "] * (end - start)
        elif operation == "m":
            pass
        elif operation == "s":
            self.saved_cursor = (self.x, self.y)
        elif operation == "u":
            self.x, self.y = self.saved_cursor
        else:
            self.unsupported.append(f"CSI {arguments}{operation}")

    def feed(self, data):
        text = self.pending + self.decoder.decode(data)
        self.pending = ""
        index = 0
        while index < len(text):
            if text[index] != "\x1b":
                self._character(text[index])
                index += 1
                continue
            if index + 1 == len(text):
                self.pending = text[index:]
                break
            if text[index + 1] == "[":
                end = index + 2
                while end < len(text) and not "@" <= text[end] <= "~":
                    end += 1
                if end == len(text):
                    self.pending = text[index:]
                    break
                self._csi(text[index + 2:end], text[end])
                index = end + 1
            elif text[index + 1] in {"7", "8"}:
                if text[index + 1] == "7":
                    self.saved_cursor = (self.x, self.y)
                else:
                    self.x, self.y = self.saved_cursor
                index += 2
            else:
                self.unsupported.append(f"ESC {text[index + 1]}")
                index += 2


if __name__ == "__main__":
    screen = VtScreen(8, 3)
    screen.feed(b"\x1b[?1049hhello\x1b[2;")
    screen.feed(b"2Hworld\x1b[1;1HX\x1b[?1049l")
    assert screen.last_alternate.splitlines()[:2] == ["Xello", " world"]
    assert not screen.unsupported
    screen.resize(4, 2)
    assert len(screen.cells) == 2 and all(len(row) == 4 for row in screen.cells)
    resized = VtScreen(120, 40)
    resized.feed(b"\x1b[?1049h\x1b[24;80HOLD")
    resized.resize(60, 20)
    resized.feed(b"\x1b[2J\x1b[2;2HNEW\x1b[?1049l")
    assert resized.last_alternate.split("\n")[1] == " NEW"
    assert len(resized.last_alternate.split("\n")) == 20
    assert not resized.unsupported
    print("VT decoder split-sequence/cursor/alternate/resize checks passed")
