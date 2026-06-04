#!/bin/bash

set -euo pipefail

spacetime publish -s local blackholio --module-path . --delete-data -y
