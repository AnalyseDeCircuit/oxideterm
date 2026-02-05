#!/bin/bash
# MapleMono-NF-CN 字体子集化脚本
# TTF → 精确裁剪 8105 汉字 → WOFF2 极致压缩

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FONT_DIR="$SCRIPT_DIR/../../public/fonts/MapleMono"
TTF_DIR="$SCRIPT_DIR/../../MapleMono-NF-CN-unhinted"
CHARS_FILE="$SCRIPT_DIR/chars_8105.txt"

# 检查依赖
if ! command -v pyftsubset &> /dev/null; then
    echo "❌ 错误: 请先安装 fonttools"
    echo "   pip install fonttools brotli"
    exit 1
fi

# 检查汉字文件
if [[ ! -f "$CHARS_FILE" ]]; then
    echo "❌ 错误: 找不到 chars_8105.txt"
    exit 1
fi

# ============================================================================
# Unicode 范围 (非汉字部分)
# ============================================================================

# 基础 ASCII + Latin
BASIC="U+0000-00FF,U+0100-024F"

# 终端必需：标点、箭头、方框绘制
TERMINAL="U+2000-206F,U+2190-21FF,U+2200-22FF,U+2500-259F,U+25A0-26FF"

# Nerd Fonts 图标 (PUA 区)
NF_ICONS="U+E000-E00A,U+E0A0-E0D7,U+E200-E2A9,U+E300-E3E3"
NF_ICONS="${NF_ICONS},U+E5FA-E6B5,U+E700-E7C5"
NF_ICONS="${NF_ICONS},U+EA60-EC1E,U+F000-F2E0,U+F300-F372,U+F400-F532"
NF_ICONS="${NF_ICONS},U+F0001-F1AF0"

# IEC + 杂项
MISC="U+23FB-23FE,U+2665,U+26A1,U+2714,U+2718,U+276F,U+2771,U+2B58"

UNICODES="${BASIC},${TERMINAL},${NF_ICONS},${MISC}"

# ============================================================================
# 字体处理
# ============================================================================
FONTS="Regular Bold Italic BoldItalic"

echo "🚀 MapleMono-NF-CN 字体子集化"
echo "   📝 精确保留 8105 汉字 + ASCII + Nerd Fonts"
echo "   🔧 TTF → 裁剪 → WOFF2 (Brotli 压缩)"
echo ""

for style in $FONTS; do
    NAME="MapleMono-NF-CN-${style}"
    INPUT="$TTF_DIR/${NAME}.ttf"
    OUTPUT="$FONT_DIR/${NAME}.woff2"
    
    if [[ ! -f "$INPUT" ]]; then
        echo "⚠️  跳过: ${NAME}.ttf (不存在)"
        continue
    fi
    
    echo "✂️  处理: ${NAME}.ttf"
    
    pyftsubset "$INPUT" \
        --output-file="$OUTPUT" \
        --flavor=woff2 \
        --text-file="$CHARS_FILE" \
        --unicodes="$UNICODES" \
        --layout-features='*' \
        --desubroutinize \
        --notdef-glyph \
        --notdef-outline \
        --recommended-glyphs \
        --name-IDs='*' \
        --name-languages='*'
    
    ORIG=$(ls -lh "$INPUT" | awk '{print $5}')
    NEW=$(ls -lh "$OUTPUT" | awk '{print $5}')
    echo "   ✨ $ORIG → $NEW"
done

echo ""
echo "✅ 完成！"
ls -lh "$FONT_DIR"/*.woff2
