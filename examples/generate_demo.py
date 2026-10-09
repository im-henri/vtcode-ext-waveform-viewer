"""Generate the deterministic, simulator-independent demo waveform."""
from pathlib import Path

path = Path(__file__).with_name("demo.vcd")
signals = [
    ("wire", 1, "!", "clk"), ("wire", 1, '"', "rst_n"),
    ("wire", 1, "#", "valid"), ("wire", 1, "$", "ready"),
    ("wire", 32, "%", "address [31:0]"), ("wire", 32, "&", "wdata [31:0]"),
    ("wire", 32, "'", "rdata [31:0]"), ("reg", 3, "(", "state [2:0]"),
    ("reg", 4, ")", "fifo_count [3:0]"), ("wire", 1, "*", "irq"),
    ("wire", 1, "+", "tx"), ("real", 64, ",", "voltage"),
]
lines = ["$date 2026-10-09 $end", "$version VTCode waveform demo $end",
         "$timescale 1 ns $end", "$scope module tb $end", "$scope module dut $end"]
lines.extend(f"$var {kind} {width} {code} {name} $end" for kind, width, code, name in signals)
lines.extend(["$scope module uart $end", "$var wire 1 ! clk_alias $end",
              "$var reg 8 - shift_reg [7:0] $end", "$var wire 1 . busy $end",
              "$upscope $end", "$upscope $end", "$upscope $end", "$enddefinitions $end"])
last = {}
for t in range(0, 1001, 5):
    cycle = t // 20
    values = {
        "!": str((t // 10) % 2), '"': str(int(t >= 60)),
        "#": str(int(t >= 100 and cycle % 6 in (0, 1, 2))),
        "$": str(int(t >= 80 and cycle % 7 != 3)),
        "%": f"b{0x40000000 + 4 * (cycle // 6):032b}",
        "&": f"b{(0xDEADBEEF + cycle * 0x10101) & 0xffffffff:032b}",
        "'": "bx" if t < 80 else "bz" if 420 <= t < 460 else f"b{(cycle * 0x13579) & 0xffffffff:032b}",
        "(": f"b{0 if t < 60 else cycle % 5:03b}",
        ")": f"b{max(0, min(15, (cycle % 24) - 3)):04b}",
        "*": str(int(cycle % 20 in (17, 18))),
        "+": str(int(t < 100 or ((cycle // 3) & 1) == 0)),
        ",": f"r{0.8 + (cycle % 16) * 0.025:.3f}",
        "-": f"b{(0xA5 >> ((cycle // 3) % 8)):08b}",
        ".": str(int(100 <= t < 700)),
    }
    lines.append(f"#{t}")
    if t == 0:
        lines.append("$dumpvars")
    for code, value in values.items():
        if last.get(code) != value:
            lines.append(f"{value} {code}" if value.startswith(("b", "r")) else f"{value}{code}")
    if t == 0:
        lines.append("$end")
    last = values
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(path)
