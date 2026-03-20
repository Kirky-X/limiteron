#!/bin/bash
# Install pre-commit hooks for Limiteron project

echo "🔧 Installing pre-commit hooks for Limiteron..."

# Install pre-commit if not installed
if ! command -v pre-commit &> /dev/null; then
    echo "📦 Installing pre-commit..."
    pip install pre-commit
fi

# Install the pre-commit hooks
echo "📥 Installing git hooks..."
pre-commit install

# Update hooks to latest versions
echo "🔄 Updating hooks..."
pre-commit autoupdate

echo "✅ Pre-commit hooks installed successfully!"
echo ""
echo "Available hooks:"
pre-commit hooks
