#pragma once

#define IMGUI_DISABLE_OBSOLETE_FUNCTIONS // match ImGui.NET

#if __has_include("godot_cpp/godot.hpp") // GDExtension
#include <godot_cpp/variant/color.hpp>
#include <godot_cpp/variant/vector2.hpp>
#include <godot_cpp/variant/vector2i.hpp>
#include <godot_cpp/variant/vector4.hpp>
using godot::Color;
using godot::Vector2;
using godot::Vector2i;
using godot::Vector4;

#if defined(DEBUG_ENABLED) && defined(IGN_EXPORT)
#ifndef IM_ASSERT
#include <godot_cpp/variant/utility_functions.hpp>
#define IM_ASSERT(_EXPR)                                                                                             \
    do                                                                                                               \
    {                                                                                                                \
        if (!(_EXPR))                                                                                                \
            godot::UtilityFunctions::push_error(godot::vformat("IM_ASSERT %s (%s:%d)", #_EXPR, __FILE__, __LINE__)); \
    } while (0)
#endif
#endif
#else // module
#include "core/math/color.h"
#include "core/math/vector2.h"
#include "core/math/vector2i.h"
#include "core/math/vector4.h"
#endif

#define IM_VEC2_CLASS_EXTRA                                                                      \
    constexpr ImVec2(const Vector2& f) : x(f.x), y(f.y)                                          \
    {                                                                                            \
    }                                                                                            \
    operator Vector2() const                                                                     \
    {                                                                                            \
        return Vector2(x, y);                                                                    \
    }                                                                                            \
    constexpr ImVec2(const Vector2i& f) : x(static_cast<float>(f.x)), y(static_cast<float>(f.y)) \
    {                                                                                            \
    }                                                                                            \
    operator Vector2i() const                                                                    \
    {                                                                                            \
        return Vector2i(static_cast<int32_t>(x), static_cast<int32_t>(y));                       \
    }

#define IM_VEC4_CLASS_EXTRA                                             \
    constexpr ImVec4(const Vector4& f) : x(f.x), y(f.y), z(f.z), w(f.w) \
    {                                                                   \
    }                                                                   \
    operator Vector4() const                                            \
    {                                                                   \
        return Vector4(x, y, z, w);                                     \
    }                                                                   \
    constexpr ImVec4(const Color& c) : x(c.r), y(c.g), z(c.b), w(c.a)   \
    {                                                                   \
    }                                                                   \
    operator Color() const                                              \
    {                                                                   \
        return Color(x, y, z, w);                                       \
    }
