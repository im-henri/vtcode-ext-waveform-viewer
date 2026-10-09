"""Generate a small register-transfer recording for exploring bus boundaries."""
from pathlib import Path

path = Path(__file__).with_name("bus_transfers.vcd")
signals = [("wire", 1, "!", "clk"), ("wire", 1, '"', "reset_n"),
           ("wire", 1, "#", "ex_valid"), ("wire", 6, "$", "ex_reg_addr [5:0]"),
           ("wire", 1, "%", "ex_reg_we"), ("wire", 32, "&", "ex_reg_wdata [31:0]"),
           ("reg", 3, "'", "state [2:0]")]
transfers = [
    (0, 0x20, 0x00000000), (30, 0x34, 0x80800000),
    (130, 0x06, 0x80800080), (210, 0x06, 0x00000000),
    (420, 0x31, 0x60108000), (500, 0x38, 0x00000000),
    (580, 0x30, 0xEDED0000), (740, 0x28, 0x60108000),
    (820, 0x30, 0x00000001), (900, 0x30, 0x601070DC),
]
lines = ["$version VTCode register-transfer example $end", "$timescale 1 ns $end",
         "$scope module tb $end", "$scope module cpu $end"]
lines.extend(f"$var {kind} {width} {code} {name} $end" for kind, width, code, name in signals)
lines.extend(["$upscope $end", "$upscope $end", "$enddefinitions $end"])
last = {}
for time in range(0, 1001, 10):
    transfer = next(item for item in reversed(transfers) if item[0] <= time)
    values = {"!": str((time // 40) % 2), '"': str(int(time >= 80)),
              "#": str(int(time >= 80 and not 340 <= time < 420)),
              "$": f"b{transfer[1]:06b}", "%": str(int(time >= 80 and not 660 <= time < 740)),
              "&": f"b{transfer[2]:032b}", "'": f"b{min(4, time // 200):03b}"}
    lines.append(f"#{time}")
    for code, value in values.items():
        if value != last.get(code):
            lines.append(f"{value} {code}" if value.startswith("b") else f"{value}{code}")
    last = values
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(path)
