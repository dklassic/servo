#!/usr/bin/env python3
import argparse
import os
from typing import Any, List, Optional, Text, TYPE_CHECKING

from . import manifest
from . import vcs
from .log import get_logger, enable_debug_logging
from .download import download_from_github
if TYPE_CHECKING:
    from .manifest import Manifest  # avoid cyclic import


here = os.path.dirname(__file__)

wpt_root = os.path.abspath(os.path.join(here, os.pardir, os.pardir))

logger = get_logger()


def update(tests_root: str,
           manifest: "Manifest",
           manifest_path: Optional[str] = None,
           working_copy: bool = True,
           cache_root: Optional[str] = None,
           paths_to_update: Optional[List[Text]] = None,
           rebuild: bool = False,
           parallel: bool = True
           ) -> bool:
    logger.warning("Deprecated; use manifest.load_and_update instead")
    logger.info("Updating manifest")

    tree = vcs.get_tree(tests_root, manifest, manifest_path, cache_root,
                        paths_to_update, working_copy, rebuild)
    return manifest.update(tree, parallel)


def update_from_cli(**kwargs: Any) -> None:
    tests_root = kwargs["tests_root"]
    path = kwargs["path"]
    assert tests_root is not None

    if not kwargs["rebuild"] and kwargs["download"]:
        download_from_github(path, tests_root)

    paths_to_update = []
    for path_to_update in kwargs["tests"]:
        if path_to_update.startswith(tests_root):
            paths_to_update.append(os.path.relpath(path_to_update, tests_root))
        else:
            logger.warning(f"{path_to_update} is not a WPT path")

    manifest.load_and_update(tests_root,
                             path,
                             kwargs["url_base"],
                             paths_to_update=paths_to_update,
                             update=True,
                             rebuild=kwargs["rebuild"],
                             cache_root=kwargs["cache_root"],
                             parallel=kwargs["parallel"])


def abs_path(path: str) -> str:
    return os.path.abspath(os.path.expanduser(path))


def create_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "-v", "--verbose", action="store_true",
        help="Turn on verbose logging")
    parser.add_argument(
        "-p", "--path", type=abs_path, help="Path to manifest file.")
    parser.add_argument(
        "--tests-root", type=abs_path, default=wpt_root, help="Path to root of tests.")
    parser.add_argument(
        "-r", "--rebuild", action="store_true",
        help="Force a full rebuild of the manifest.")
    parser.add_argument(
        "--url-base", default="/",
        help="Base url to use as the mount point for tests in this manifest.")
    parser.add_argument(
        "--no-download", dest="download", action="store_false",
        help="Never attempt to download the manifest.")
    parser.add_argument(
        "--cache-root", default=os.path.join(wpt_root, ".wptcache"),
        help="Path in which to store any caches (default <tests_root>/.wptcache/)")
    parser.add_argument(
        "--no-parallel", dest="parallel", action="store_false",
        help="Do not parallelize building the manifest")
    parser.add_argument('tests',
                        type=abs_path,
                        nargs='*',
                        help=('Test files or directories to update. '
                              'Omit to update all items under the test root.'))
    return parser


def run(*args: Any, **kwargs: Any) -> None:
    if kwargs["path"] is None:
        kwargs["path"] = os.path.join(kwargs["tests_root"], "MANIFEST.json")
    if kwargs["verbose"]:
        enable_debug_logging()
    update_from_cli(**kwargs)


def main() -> None:
    opts = create_parser().parse_args()

    run(**vars(opts))
