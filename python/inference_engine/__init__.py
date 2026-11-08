"""Alpha native rule-profile compilation; graph evaluation is not implemented yet."""
import json
from . import _native

__version__ = _native.__version__

def profiles():
    """Return the exact available shared rule-profile selectors."""
    return _native.profiles()

def compile_profile(selector):
    """Compile a shared profile into portable rules with explicit exclusions."""
    return json.loads(_native.compile_profile(selector))
