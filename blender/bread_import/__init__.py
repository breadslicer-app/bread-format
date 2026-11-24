# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""Imports .bread files into Blender, from File > Import > Bread (.bread) or
by dropping one on the 3D Viewport or the Outliner, and exports them, from
File > Export > Bread (.bread). Blender is imported only when the extension
registers, so the rest can be tested without it."""


def register():
    from . import exporter, importer

    importer.register()
    exporter.register()


def unregister():
    from . import exporter, importer

    exporter.unregister()
    importer.unregister()
