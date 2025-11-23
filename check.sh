#!/bin/sh
echo "Cheking bugs..."
cargo clippy
echo "Formatting codes..."
cargo fmt
echo "Final check:"
cargo check
echo "Show project status (Git):"
git status

