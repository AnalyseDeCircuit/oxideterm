#!/bin/bash
# JetBrainsMono & Meslo 西文字体重压缩脚本
# 使用 --desubroutinize + Brotli 级别 9 优化

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR/../.."

# 检查依赖
if ! command -v pyftsubset &> /dev/null; then
    echo "❌ 错误: 请先安装 fonttools"
    echo "   pip install fonttools brotli"
    exit 1
fi

# ============================================================================
# 西文字体不需要汉字，只保留基础字符 + Nerd Fonts 图标
# ============================================================================

# 基础 ASCII + Latin + 扩展拉丁
LATIN="U+0000-00FF,U+0100-024F,U+0250-02AF"

# 终端必需：标点、箭头、方框、几何图形
TERMINAL="U+2000-206F,U+2190-21FF,U+2200-22FF,U+2500-259F,U+25A0-26FF"

# Nerd Fonts 图标 (完整覆盖)
NF_ICONS="U+E000-E00A,U+E0A0-E0D7,U+E200-E2A9,U+E300-E3E3"
NF_ICONS="${NF_ICONS},U+E5FA-E6B5,U+E700-E7C5"
NF_ICONS="${NF_ICONS},U+EA60-EC1E,U+F000-F2E0,U+F300-F372,U+F400-F532"

# 杂项符号
MISC="U+23FB-23FE,U+2665,U+26A1,U+2714,U+2718,U+276F,U+2771,U+2B58"

UNICODES="${LATIN},${TERMINAL},${NF_ICONS},${MISC}"

# ============================================================================
# 处理 JetBrainsMono
# ============================================================================
echo "🚀 优化 JetBrainsMono 字体"
echo "   🔧 TTF → 裁剪 → WOFF2 (Brotli Level 9 + Desubroutinize)"
echo ""

JBM_TTF_DIR="$PROJECT_ROOT/JetBrainsMono"
JBM_OUTPUT_DIR="$PROJECT_ROOT/public/fonts/JetBrainsMono"

for style in Regular Bold Italic BoldItalic; do
    # 注意：需要匹配实际的 TTF 文件名模式
    INPUT=$(find "$JBM_TTF_DIR" -name "*Mono-${style}.ttf" -o -name "*NerdFontMono-${style}.ttf" | head -1)
    
    if [[ -z "$INPUT" || ! -f "$INPUT" ]]; then
        echo "⚠️  跳过: JetBrainsMono ${style} (TTF 不存在)"
        continue
    fi
    
    OUTPUT="$JBM_OUTPUT_DIR/JetBrainsMonoNerdFontMono-${style}.woff2"
    
    echo "✂️  处理: $(basename "$INPUT")"
    
    pyftsubset "$INPUT" \
        --output-file="$OUTPUT" \
        --flavor=woff2 \
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

# ============================================================================
# 处理 Meslo
# ============================================================================
echo ""
echo "🚀 优化 Meslo 字体"
echo "   🔧 TTF → 裁剪 → WOFF2 (Brotli Level 9 + Desubroutinize)"
echo ""

MESLO_TTF_DIR="$PROJECT_ROOT/Meslo"
MESLO_OUTPUT_DIR="$PROJECT_ROOT/public/fonts/Meslo"

for style in Regular Bold Italic BoldItalic; do
    # Meslo 文件名格式：MesloLGLDZNerdFontMono-Regular.ttf
    INPUT=$(find "$MESLO_TTF_DIR" -name "*Mono-${style}.ttf" | grep -i "LGLDZ" | head -1)
    
    if [[ -z "$INPUT" || ! -f "$INPUT" ]]; then
        echo "⚠️  跳过: Meslo ${style} (TTF 不存在)"
        continue
    fi
    
    OUTPUT="$MESLO_OUTPUT_DIR/MesloLGMNerdFontMono-${style}.woff2"
    
    echo "✂️  处理: $(basename "$INPUT")"
    
    pyftsubset "$INPUT" \
        --output-file="$OUTPUT" \
        --flavor=woff2 \
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
echo ""
echo "📊 最终大小："
ls -lh "$JBM_OUTPUT_DIR"/*.woff2
echo ""
ls -lh "$MESLO_OUTPUT_DIR"/*.woff2
