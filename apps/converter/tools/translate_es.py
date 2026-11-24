# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0
"""Writes i18n/es/LC_MESSAGES/bread-converter.po from i18n/bread-converter.pot
(made with `slint-tr-extractor ui/app.slint -o i18n/bread-converter.pot`) and
the Spanish below. Fails on a text it has no Spanish for."""

import re
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent

ES = {
    "Already there: {}": "Ya existe: {}",
    "Not converted this way": "No se convierte en este sentido",
    "→ {}": "→ {}",
    "Waiting": "En espera",
    "Converting…": "Convirtiendo…",
    "Checked": "Verificado",
    "Converted": "Convertido",
    "Skipped": "Salteado",
    "Failed": "Falló",
    "{n} file": ("{n} archivo", "{n} archivos"),
    "{} converted, {} skipped, {} failed": "{} convertidos, {} salteados, {} con error",
    "Nothing to add there: no STL or .bread files.": "No hay nada para agregar ahí: ni STL ni .bread.",
    "Choose the folder to write into.": "Elegí la carpeta donde escribir.",
    "Converting {} of {}: {} of {}, {}": "Convirtiendo {} de {}: {} de {}, {}",
    "Done in {}: {}": "Listo en {}: {}",
    "Stopped after {}: {}": "Detenido a los {}: {}",
    "Add STL or .bread files to convert.": "Agregá archivos STL o .bread para convertir.",
    "{}, {}": "{}, {}",
    "STL and .bread, both ways, without losing a bit": "STL y .bread, en los dos sentidos, sin perder un bit",
    "About": "Acerca de",
    "ADD": "AGREGAR",
    "Files…": "Archivos…",
    "Folder…": "Carpeta…",
    "Look inside subfolders": "Buscar también en subcarpetas",
    "Or drop files and folders on the list.": "O soltá archivos y carpetas sobre la lista.",
    "CONVERT": "CONVERTIR",
    "Both ways": "Los dos",
    "STL → .bread": "STL → .bread",
    ".bread → STL": ".bread → STL",
    "Each file by its extension: STL to .bread, .bread to STL.": "Cada archivo según su extensión: STL a .bread, .bread a STL.",
    "STL files only, binary or ASCII. Each .bread file is read back and checked bit for bit before it is written.":
        "Solo archivos STL, binarios o ASCII. Cada .bread se vuelve a leer y se verifica bit a bit antes de escribirlo.",
    ".bread files only. Everything each file shows goes into one STL, in place.":
        "Solo archivos .bread. Todo lo que muestra cada uno va a un STL, en su lugar.",
    "WRITE": "ESCRIBIR",
    "Next to each file": "Al lado de cada uno",
    "Into a folder": "En una carpeta",
    "Choose a folder": "Elegí una carpeta",
    "Keep the subfolders": "Mantener las subcarpetas",
    "IF THE FILE IS THERE": "SI EL ARCHIVO YA EXISTE",
    "Skip": "Saltear",
    "Replace": "Reemplazar",
    "Keep both": "Dejar los dos",
    "converted": "convertidos",
    "time": "tiempo",
    "before": "antes",
    "after": "después",
    "FILE": "ARCHIVO",
    "WAY": "SENTIDO",
    "SIZE": "TAMAÑO",
    "RESULT": "RESULTADO",
    "STATE": "ESTADO",
    "Drop STL and .bread files or folders here": "Soltá acá archivos STL y .bread, o carpetas",
    "or add them with Files… and Folder…": "o agregalos con Archivos… y Carpeta…",
    "Drop to add": "Soltá para agregar",
    "Open folder": "Abrir carpeta",
    "Clear": "Vaciar",
    "Stop": "Detener",
    "Convert": "Convertir",
    "Close": "Cerrar",
    "Ready": "Listo",
    "Converted in {}, nothing written: {} would take {}, {} more.": "Convertido en {}, sin escribir nada: {} ocuparían {}, {} más.",
    "Converted in {}, nothing written: {} would take {}, {} less.": "Convertido en {}, sin escribir nada: {} ocuparían {}, {} menos.",
    "EXPORT": "EXPORTAR",
    "Convert and export": "Convertir y exportar",
    "Convert only": "Solo convertir",
    "Each file is written as soon as it is converted.": "Cada archivo se escribe apenas se convierte.",
    "Nothing is written yet: see how much room they would take, then export them as set below.":
        "Todavía no se escribe nada: mirá cuánto lugar ocuparían y después exportalos como se elija abajo.",
    "would take": "ocuparían",
    "more": "más",
    "saved": "ahorrado",
    "free where it would go": "libre donde irían",
    "There is not enough room there for them: export them somewhere else, or skip some.": "No hay lugar suficiente ahí: exportalos a otro lado, o dejá algunos afuera.",
    "Export": "Exportar",
    "Minimize": "Minimizar",
    "Maximize": "Maximizar",
    "Restore": "Restaurar",
    "Part of bread-format, the open .bread format: tiny, lossless meshes and scenes. Apache-2.0. It carries meshoptimizer (MIT) and Zstandard (BSD), and the Noto Sans font (SIL OFL 1.1). Its window is drawn with Slint.":
        "Parte de bread-format, el formato abierto .bread: mallas y escenas chiquitas y sin pérdida. Apache-2.0. Incluye meshoptimizer (MIT) y Zstandard (BSD), y la fuente Noto Sans (SIL OFL 1.1). Su ventana está hecha con Slint.",
    "“{}” · {}": "“{}” · {}",
    "CREDIT": "CRÉDITO",
    "Designer": "Quién lo diseñó",
    "License, such as CC-BY-4.0": "Licencia, como CC-BY-4.0",
    "Where they come from: https://…": "De dónde vienen: https://…",
    "Written into every .bread file, with the file's name as its title: click a file to change it. What is left empty is not written.":
        "Se escribe en cada archivo .bread, con el nombre del archivo como título: hacé clic en un archivo para cambiarlo. Lo que quede vacío no se escribe.",
    "Title": "Título",
    "The title written into this file's .bread. Left empty, it has none.": "El título que se escribe en el .bread de este archivo. Si queda vacío, no tiene.",
    "From the file's name": "Del nombre del archivo",
    "OK": "Aceptar",
}


def unquote(lines):
    return "".join(re.match(r'^(?:msgid|msgid_plural|msgctxt)?\s*"(.*)"$', l).group(1) for l in lines).replace('\\"', '"')


def quote(s):
    return '"' + s.replace('"', '\\"') + '"'


def main():
    pot = (HERE / "i18n" / "bread-converter.pot").read_text(encoding="utf-8")
    out = [
        'msgid ""', 'msgstr ""', r'"Language: es\n"', r'"MIME-Version: 1.0\n"',
        r'"Content-Type: text/plain; charset=UTF-8\n"', r'"Content-Transfer-Encoding: 8bit\n"',
        r'"Plural-Forms: nplurals=2; plural=n != 1;\n"', "",
    ]
    missing = []
    for block in pot.split("\n\n"):
        lines = block.strip().splitlines()
        keep = [l for l in lines if l.startswith(("#", "msgctxt"))]
        fields = {}
        current = None
        for l in lines:
            m = re.match(r"^(msgctxt|msgid_plural|msgid|msgstr(?:\[\d\])?)\s", l)
            if m:
                current = m.group(1)
                fields[current] = [l]
            elif l.startswith('"') and current:
                fields[current].append(l)
        if "msgid" not in fields:
            continue
        msgid = unquote(fields["msgid"])
        if msgid == "":
            continue
        es = ES.get(msgid)
        if es is None:
            missing.append(msgid)
            continue
        out += keep + fields["msgid"]
        if "msgid_plural" in fields:
            out += fields["msgid_plural"]
            out += [f"msgstr[0] {quote(es[0])}", f"msgstr[1] {quote(es[1])}"]
        else:
            out.append(f"msgstr {quote(es)}")
        out.append("")
    if missing:
        raise SystemExit("no Spanish for: " + repr(missing))
    target = HERE / "i18n" / "es" / "LC_MESSAGES" / "bread-converter.po"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text("\n".join(out), encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
