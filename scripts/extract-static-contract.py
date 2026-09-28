#!/usr/bin/env python3
"""Export interoperability metadata only; requires dnfile==0.18.0.

Usage: python scripts/extract-static-contract.py /path/to/POSServer.exe output.json
Input must be extracted from the installer pinned in the profile manifest.
No binary, certificates, executable implementation, or help prose is exported.
"""

import argparse
import hashlib
import json
from pathlib import Path

import dnfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("assembly")
parser.add_argument("output")
args = parser.parse_args()
pe = dnfile.dnPE(args.assembly)
method_types = {}
for t in pe.net.mdtables.TypeDef.rows:
    for m in t.MethodList:
        method_types[m.row_index] = f"{t.TypeNamespace}.{t.TypeName}"


def attr_name(a):
    c = a.Type.row
    return (
        str(c.Class.row.TypeName)
        if hasattr(c, "Class") and hasattr(c.Class.row, "TypeName")
        else method_types.get(a.Type.row_index, str(c.Name))
    )


def serstr(b, off=2):
    n = b[off]
    off += 1
    if n == 255:
        return None, off
    if n & 128:
        if n & 64:
            n = ((n & 31) << 24) | (b[off] << 16) | (b[off + 1] << 8) | b[off + 2]
            off += 3
        else:
            n = ((n & 63) << 8) | b[off]
            off += 1
    return b[off : off + n].decode("utf-8"), off + n


statuses = []
for f in next(
    t for t in pe.net.mdtables.TypeDef.rows if str(t.TypeName) == "TerminalStatuses"
).FieldList:
    constants = [
        c
        for c in pe.net.mdtables.Constant.rows
        if c.Parent.table.name == "Field" and c.Parent.row_index == f.row_index
    ]
    if constants:
        statuses.append(
            {
                "name": str(f.row.Name),
                "code": int.from_bytes(constants[0].Value.value, "little", signed=True),
            }
        )
routes = []
for a in pe.net.mdtables.CustomAttribute.rows:
    p = a.Parent
    n = attr_name(a)
    if (
        p.table.name == "MethodDef"
        and method_types[p.row_index] == "POSServer.Controllers.POSController"
        and n
        in [
            "HttpGetAttribute",
            "HttpPostAttribute",
            "HttpDeleteAttribute",
            "HttpPutAttribute",
            "RouteAttribute",
        ]
    ):
        routes.append(
            {
                "method": str(p.row.Name),
                "attribute": n,
                "template": serstr(a.Value.value)[0],
            }
        )
    if (
        p.table.name == "TypeDef"
        and str(p.row.TypeName) == "POSController"
        and n == "RouteAttribute"
    ):
        print("Controller route:", serstr(a.Value.value)[0])
selected = {
    "CommonClasses.BaseResponseDTO",
    "CommonClasses.ErrorResponse",
    "CommonClasses.PaymentDTO",
    "CommonClasses.PerformPurchaseDTO",
    "CommonClasses.ResponseDTO",
    "CommonClasses.DeviceDTO",
    "CommonClasses.Devices.POSDeviceDTO",
    "CommonClasses.Response.ValidationError",
    "CommonClasses.Response.HTTPValidationError",
}
props = []
for pm in pe.net.mdtables.PropertyMap.rows:
    t = pm.Parent.row
    owner = f"{t.TypeNamespace}.{t.TypeName}"
    if owner not in selected:
        continue
    for p in pm.PropertyList:
        ats = []
        for a in pe.net.mdtables.CustomAttribute.rows:
            if a.Parent.table.name != "Property" or a.Parent.row_index != p.row_index:
                continue
            n = attr_name(a)
            if n == "JsonPropertyAttribute":
                name, off = serstr(a.Value.value)
                ats.append(
                    {
                        "attribute": n,
                        "json_name": name,
                        "metadata_hex": a.Value.value.hex(),
                    }
                )
            elif n in ["JsonIgnoreAttribute", "JsonExtensionDataAttribute"]:
                ats.append({"attribute": n})
            elif n == "JsonConverterAttribute":
                ats.append(
                    {
                        "attribute": n,
                        "converter": serstr(a.Value.value)[0].split(",")[0],
                    }
                )
        props.append(
            {
                "type": owner,
                "property": str(p.row.Name),
                "signature_hex": p.row.Type.value.hex(),
                "json_attributes": ats,
            }
        )
data = {
    "profile": "desktop-paylink-2.1.20-win-x86",
    "evidence": "static CLR metadata, not live reference capture",
    "assembly": "POSServer.exe",
    "assembly_sha256": hashlib.sha256(Path(args.assembly).read_bytes()).hexdigest(),
    "terminal_statuses": statuses,
    "routes": routes,
    "dto_properties": props,
}


for prop in data["dto_properties"]:
    for a in prop["json_attributes"]:
        if "metadata_hex" not in a:
            continue
        b = bytes.fromhex(a.pop("metadata_hex"))
        name, off = serstr(b)
        count = int.from_bytes(b[off : off + 2], "little")
        off += 2
        opts = {}
        for _ in range(count):
            assert b[off] in [83, 84]
            off += 1
            typ = b[off]
            off += 1
            if typ == 85:
                enum, off = serstr(b, off)
            key, off = serstr(b, off)
            if typ == 85 or typ == 8:
                val = int.from_bytes(b[off : off + 4], "little", signed=True)
                off += 4
            elif typ == 2:
                val = bool(b[off])
                off += 1
            elif typ == 14:
                val, off = serstr(b, off)
            else:
                raise ValueError((typ, key))
            opts[key] = val
        assert off == len(b), (prop, off, len(b))
        a["options"] = opts

expected_sha = "85d1bf9280e7c63e1099ecc2db480813f439e68c41a5eca8a453094a948ab465"
if data["assembly_sha256"] != expected_sha:
    raise SystemExit("Assembly hash differs from the pinned 2.1.20 reference")
Path(args.output).write_text(json.dumps(data, indent=2) + "\n")
print(
    f"Exported {len(statuses)} statuses, {len(routes)} route attributes and {len(props)} properties"
)
