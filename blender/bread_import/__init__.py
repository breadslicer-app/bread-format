# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""Imports .bread files into Blender: File > Import > Bread (.bread), or drop
one on the 3D Viewport or the Outliner. Blender is imported only when the
extension registers, so the rest can be tested without it."""


def register():
    from . import importer

    importer.register()


def unregister():
    from . import importer

    importer.unregister()
