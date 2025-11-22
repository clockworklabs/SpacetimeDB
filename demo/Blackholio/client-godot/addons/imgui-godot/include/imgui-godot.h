#pragma once
#include <imgui.h>

#ifndef IMGUI_HAS_VIEWPORT
#error use ImGui docking branch
#endif

#if __has_include("godot_cpp/godot.hpp")
#define IGN_GDEXT
// GDExtension
#include <godot_cpp/classes/atlas_texture.hpp>
#include <godot_cpp/classes/engine.hpp>
#include <godot_cpp/classes/font_file.hpp>
#include <godot_cpp/classes/input_event.hpp>
#include <godot_cpp/classes/resource.hpp>
#include <godot_cpp/classes/sub_viewport.hpp>
#include <godot_cpp/classes/texture2d.hpp>
#include <godot_cpp/classes/window.hpp>
#include <godot_cpp/variant/callable.hpp>
#include <godot_cpp/variant/typed_array.hpp>
#else
// module
#include "core/config/engine.h"
#include "core/input/input_enums.h"
#include "core/os/keyboard.h"
#include "core/variant/callable.h"
#include "scene/main/viewport.h"
#include "scene/main/window.h"
#include "scene/resources/atlas_texture.h"
#include "scene/resources/texture.h"
#endif

static_assert(sizeof(void*) == 8);
static_assert(sizeof(ImDrawIdx) == 2);
static_assert(sizeof(ImWchar) == 2);

namespace ImGui::Godot {

#if defined(IGN_GDEXT)
using godot::AtlasTexture;
using godot::Callable;
using godot::ClassDB;
using godot::Color;
using godot::Engine;
using godot::FontFile;
using godot::JoyButton;
using godot::Key;
using godot::Object;
using godot::PackedInt32Array;
using godot::Ref;
using godot::RID;
using godot::String;
using godot::StringName;
using godot::Texture2D;
using godot::TypedArray;
using godot::Vector2;
using godot::Viewport;
#endif

static_assert(sizeof(RID) == 8);
#ifndef IGN_EXPORT
// C++ user interface
namespace detail {
inline static Object* ImGuiGD = nullptr;

inline bool GET_IMGUIGD()
{
    if (ImGuiGD)
        return true;
#ifdef IGN_GDEXT
    ImGuiGD = Engine::get_singleton()->get_singleton("ImGuiGD");
#else
    ImGuiGD = Engine::get_singleton()->get_singleton_object("ImGuiGD");
#endif
    return ImGuiGD != nullptr;
}

inline static void GetAtlasUVs(AtlasTexture* tex, ImVec2& uv0, ImVec2& uv1)
{
    ERR_FAIL_COND(!tex);
    Vector2 atlasSize = tex->get_atlas()->get_size();
    uv0 = tex->get_region().get_position() / atlasSize;
    uv1 = tex->get_region().get_end() / atlasSize;
}
} // namespace detail

inline void AddFont(const Ref<FontFile>& fontFile, int fontSize, bool merge = false, ImWchar* glyphRanges = nullptr)
{
    ERR_FAIL_COND(!detail::GET_IMGUIGD());
    static const StringName sn("AddFont");
    PackedInt32Array gr;
    if (glyphRanges)
    {
        do
        {
            gr.append(*glyphRanges);
        } while (*++glyphRanges != 0);
    }
    detail::ImGuiGD->call(sn, fontSize, merge, gr);
}

inline void Connect(const Callable& callable)
{
    ERR_FAIL_COND(!detail::GET_IMGUIGD());
    static const StringName sn("Connect");
    detail::ImGuiGD->call(sn, callable);
}

inline void RebuildFontAtlas()
{
    ERR_FAIL_COND(!detail::GET_IMGUIGD());
    static const StringName sn("RebuildFontAtlas");
    detail::ImGuiGD->call(sn);
}

inline void ResetFonts()
{
    ERR_FAIL_COND(!detail::GET_IMGUIGD());
    static const StringName sn("ResetFonts");
    detail::ImGuiGD->call(sn);
}

inline void SetJoyAxisDeadZone(real_t deadZone)
{
    ERR_FAIL_COND(!detail::GET_IMGUIGD());
    static const StringName sn("JoyAxisDeadZone");
    detail::ImGuiGD->set(sn, deadZone);
}

inline void SetVisible(bool vis)
{
    ERR_FAIL_COND(!detail::GET_IMGUIGD());
    static const StringName sn("Visible");
    detail::ImGuiGD->set(sn, vis);
}

inline void SetMainViewport(Viewport* vp)
{
    ERR_FAIL_COND(!detail::GET_IMGUIGD());
    static const StringName sn("SetMainViewport");
    detail::ImGuiGD->call(sn, vp);
}

inline bool ToolInit()
{
    ERR_FAIL_COND_V(!detail::GET_IMGUIGD(), false);
    static const StringName sn("ToolInit");
    return detail::ImGuiGD->call(sn);
}

inline void SetIniFilename(String fn)
{
    ERR_FAIL_COND(!detail::GET_IMGUIGD());
    static const StringName sn("SetIniFilename");
    detail::ImGuiGD->call(sn, fn);
}

inline void SyncImGuiPtrs()
{
    Object* obj = ClassDB::instantiate("ImGuiSync");
    ERR_FAIL_COND(!obj);

    static const StringName sn("GetImGuiPtrs");
    TypedArray<int64_t> ptrs = obj->call(sn,
                                         String(ImGui::GetVersion()),
                                         (int32_t)sizeof(ImGuiIO),
                                         (int32_t)sizeof(ImDrawVert),
                                         (int32_t)sizeof(ImDrawIdx),
                                         (int32_t)sizeof(ImWchar));

    ERR_FAIL_COND(ptrs.size() != 3);

    ImGui::SetCurrentContext(reinterpret_cast<ImGuiContext*>((int64_t)ptrs[0]));
    ImGuiMemAllocFunc alloc_func = reinterpret_cast<ImGuiMemAllocFunc>((int64_t)ptrs[1]);
    ImGuiMemFreeFunc free_func = reinterpret_cast<ImGuiMemFreeFunc>((int64_t)ptrs[2]);
    ImGui::SetAllocatorFunctions(alloc_func, free_func, nullptr);
    memdelete(obj);
}

inline ImTextureID BindTexture(Texture2D* tex)
{
    ERR_FAIL_COND_V(!tex, 0);
    return reinterpret_cast<ImTextureID>(tex->get_rid().get_id());
}
#endif

#ifdef IGN_GDEXT // GDExtension
inline ImGuiKey ToImGuiKey(Key key)
{
    switch (key)
    {
    case Key::KEY_ESCAPE:
        return ImGuiKey_Escape;
    case Key::KEY_TAB:
        return ImGuiKey_Tab;
    case Key::KEY_BACKSPACE:
        return ImGuiKey_Backspace;
    case Key::KEY_ENTER:
        return ImGuiKey_Enter;
    case Key::KEY_KP_ENTER:
        return ImGuiKey_KeypadEnter;
    case Key::KEY_INSERT:
        return ImGuiKey_Insert;
    case Key::KEY_DELETE:
        return ImGuiKey_Delete;
    case Key::KEY_PAUSE:
        return ImGuiKey_Pause;
    case Key::KEY_PRINT:
        return ImGuiKey_PrintScreen;
    case Key::KEY_HOME:
        return ImGuiKey_Home;
    case Key::KEY_END:
        return ImGuiKey_End;
    case Key::KEY_LEFT:
        return ImGuiKey_LeftArrow;
    case Key::KEY_UP:
        return ImGuiKey_UpArrow;
    case Key::KEY_RIGHT:
        return ImGuiKey_RightArrow;
    case Key::KEY_DOWN:
        return ImGuiKey_DownArrow;
    case Key::KEY_PAGEUP:
        return ImGuiKey_PageUp;
    case Key::KEY_PAGEDOWN:
        return ImGuiKey_PageDown;
    case Key::KEY_SHIFT:
        return ImGuiKey_LeftShift;
    case Key::KEY_CTRL:
        return ImGuiKey_LeftCtrl;
    case Key::KEY_META:
        return ImGuiKey_LeftSuper;
    case Key::KEY_ALT:
        return ImGuiKey_LeftAlt;
    case Key::KEY_CAPSLOCK:
        return ImGuiKey_CapsLock;
    case Key::KEY_NUMLOCK:
        return ImGuiKey_NumLock;
    case Key::KEY_SCROLLLOCK:
        return ImGuiKey_ScrollLock;
    case Key::KEY_F1:
        return ImGuiKey_F1;
    case Key::KEY_F2:
        return ImGuiKey_F2;
    case Key::KEY_F3:
        return ImGuiKey_F3;
    case Key::KEY_F4:
        return ImGuiKey_F4;
    case Key::KEY_F5:
        return ImGuiKey_F5;
    case Key::KEY_F6:
        return ImGuiKey_F6;
    case Key::KEY_F7:
        return ImGuiKey_F7;
    case Key::KEY_F8:
        return ImGuiKey_F8;
    case Key::KEY_F9:
        return ImGuiKey_F9;
    case Key::KEY_F10:
        return ImGuiKey_F10;
    case Key::KEY_F11:
        return ImGuiKey_F11;
    case Key::KEY_F12:
        return ImGuiKey_F12;
    case Key::KEY_KP_MULTIPLY:
        return ImGuiKey_KeypadMultiply;
    case Key::KEY_KP_DIVIDE:
        return ImGuiKey_KeypadDivide;
    case Key::KEY_KP_SUBTRACT:
        return ImGuiKey_KeypadSubtract;
    case Key::KEY_KP_PERIOD:
        return ImGuiKey_KeypadDecimal;
    case Key::KEY_KP_ADD:
        return ImGuiKey_KeypadAdd;
    case Key::KEY_KP_0:
        return ImGuiKey_Keypad0;
    case Key::KEY_KP_1:
        return ImGuiKey_Keypad1;
    case Key::KEY_KP_2:
        return ImGuiKey_Keypad2;
    case Key::KEY_KP_3:
        return ImGuiKey_Keypad3;
    case Key::KEY_KP_4:
        return ImGuiKey_Keypad4;
    case Key::KEY_KP_5:
        return ImGuiKey_Keypad5;
    case Key::KEY_KP_6:
        return ImGuiKey_Keypad6;
    case Key::KEY_KP_7:
        return ImGuiKey_Keypad7;
    case Key::KEY_KP_8:
        return ImGuiKey_Keypad8;
    case Key::KEY_KP_9:
        return ImGuiKey_Keypad9;
    case Key::KEY_MENU:
        return ImGuiKey_Menu;
    case Key::KEY_SPACE:
        return ImGuiKey_Space;
    case Key::KEY_APOSTROPHE:
        return ImGuiKey_Apostrophe;
    case Key::KEY_COMMA:
        return ImGuiKey_Comma;
    case Key::KEY_MINUS:
        return ImGuiKey_Minus;
    case Key::KEY_PERIOD:
        return ImGuiKey_Period;
    case Key::KEY_SLASH:
        return ImGuiKey_Slash;
    case Key::KEY_0:
        return ImGuiKey_0;
    case Key::KEY_1:
        return ImGuiKey_1;
    case Key::KEY_2:
        return ImGuiKey_2;
    case Key::KEY_3:
        return ImGuiKey_3;
    case Key::KEY_4:
        return ImGuiKey_4;
    case Key::KEY_5:
        return ImGuiKey_5;
    case Key::KEY_6:
        return ImGuiKey_6;
    case Key::KEY_7:
        return ImGuiKey_7;
    case Key::KEY_8:
        return ImGuiKey_8;
    case Key::KEY_9:
        return ImGuiKey_9;
    case Key::KEY_SEMICOLON:
        return ImGuiKey_Semicolon;
    case Key::KEY_EQUAL:
        return ImGuiKey_Equal;
    case Key::KEY_A:
        return ImGuiKey_A;
    case Key::KEY_B:
        return ImGuiKey_B;
    case Key::KEY_C:
        return ImGuiKey_C;
    case Key::KEY_D:
        return ImGuiKey_D;
    case Key::KEY_E:
        return ImGuiKey_E;
    case Key::KEY_F:
        return ImGuiKey_F;
    case Key::KEY_G:
        return ImGuiKey_G;
    case Key::KEY_H:
        return ImGuiKey_H;
    case Key::KEY_I:
        return ImGuiKey_I;
    case Key::KEY_J:
        return ImGuiKey_J;
    case Key::KEY_K:
        return ImGuiKey_K;
    case Key::KEY_L:
        return ImGuiKey_L;
    case Key::KEY_M:
        return ImGuiKey_M;
    case Key::KEY_N:
        return ImGuiKey_N;
    case Key::KEY_O:
        return ImGuiKey_O;
    case Key::KEY_P:
        return ImGuiKey_P;
    case Key::KEY_Q:
        return ImGuiKey_Q;
    case Key::KEY_R:
        return ImGuiKey_R;
    case Key::KEY_S:
        return ImGuiKey_S;
    case Key::KEY_T:
        return ImGuiKey_T;
    case Key::KEY_U:
        return ImGuiKey_U;
    case Key::KEY_V:
        return ImGuiKey_V;
    case Key::KEY_W:
        return ImGuiKey_W;
    case Key::KEY_X:
        return ImGuiKey_X;
    case Key::KEY_Y:
        return ImGuiKey_Y;
    case Key::KEY_Z:
        return ImGuiKey_Z;
    case Key::KEY_BRACKETLEFT:
        return ImGuiKey_LeftBracket;
    case Key::KEY_BACKSLASH:
        return ImGuiKey_Backslash;
    case Key::KEY_BRACKETRIGHT:
        return ImGuiKey_RightBracket;
    case Key::KEY_QUOTELEFT:
        return ImGuiKey_GraveAccent;
    default:
        return ImGuiKey_None;
    };
}

inline ImGuiKey ToImGuiKey(JoyButton btn)
{
    switch (btn)
    {
    case JoyButton::JOY_BUTTON_A:
        return ImGuiKey_GamepadFaceDown;
    case JoyButton::JOY_BUTTON_B:
        return ImGuiKey_GamepadFaceRight;
    case JoyButton::JOY_BUTTON_X:
        return ImGuiKey_GamepadFaceLeft;
    case JoyButton::JOY_BUTTON_Y:
        return ImGuiKey_GamepadFaceUp;
    case JoyButton::JOY_BUTTON_BACK:
        return ImGuiKey_GamepadBack;
    case JoyButton::JOY_BUTTON_START:
        return ImGuiKey_GamepadStart;
    case JoyButton::JOY_BUTTON_LEFT_STICK:
        return ImGuiKey_GamepadL3;
    case JoyButton::JOY_BUTTON_RIGHT_STICK:
        return ImGuiKey_GamepadR3;
    case JoyButton::JOY_BUTTON_LEFT_SHOULDER:
        return ImGuiKey_GamepadL1;
    case JoyButton::JOY_BUTTON_RIGHT_SHOULDER:
        return ImGuiKey_GamepadR1;
    case JoyButton::JOY_BUTTON_DPAD_UP:
        return ImGuiKey_GamepadDpadUp;
    case JoyButton::JOY_BUTTON_DPAD_DOWN:
        return ImGuiKey_GamepadDpadDown;
    case JoyButton::JOY_BUTTON_DPAD_LEFT:
        return ImGuiKey_GamepadDpadLeft;
    case JoyButton::JOY_BUTTON_DPAD_RIGHT:
        return ImGuiKey_GamepadDpadRight;
    default:
        return ImGuiKey_None;
    };
}
#else // module
inline ImGuiKey ToImGuiKey(Key key)
{
    switch (key)
    {
    case Key::ESCAPE:
        return ImGuiKey_Escape;
    case Key::TAB:
        return ImGuiKey_Tab;
    case Key::BACKSPACE:
        return ImGuiKey_Backspace;
    case Key::ENTER:
        return ImGuiKey_Enter;
    case Key::KP_ENTER:
        return ImGuiKey_KeypadEnter;
    case Key::INSERT:
        return ImGuiKey_Insert;
    case Key::KEY_DELETE:
        return ImGuiKey_Delete;
    case Key::PAUSE:
        return ImGuiKey_Pause;
    case Key::PRINT:
        return ImGuiKey_PrintScreen;
    case Key::HOME:
        return ImGuiKey_Home;
    case Key::END:
        return ImGuiKey_End;
    case Key::LEFT:
        return ImGuiKey_LeftArrow;
    case Key::UP:
        return ImGuiKey_UpArrow;
    case Key::RIGHT:
        return ImGuiKey_RightArrow;
    case Key::DOWN:
        return ImGuiKey_DownArrow;
    case Key::PAGEUP:
        return ImGuiKey_PageUp;
    case Key::PAGEDOWN:
        return ImGuiKey_PageDown;
    case Key::SHIFT:
        return ImGuiKey_LeftShift;
    case Key::CTRL:
        return ImGuiKey_LeftCtrl;
    case Key::META:
        return ImGuiKey_LeftSuper;
    case Key::ALT:
        return ImGuiKey_LeftAlt;
    case Key::CAPSLOCK:
        return ImGuiKey_CapsLock;
    case Key::NUMLOCK:
        return ImGuiKey_NumLock;
    case Key::SCROLLLOCK:
        return ImGuiKey_ScrollLock;
    case Key::F1:
        return ImGuiKey_F1;
    case Key::F2:
        return ImGuiKey_F2;
    case Key::F3:
        return ImGuiKey_F3;
    case Key::F4:
        return ImGuiKey_F4;
    case Key::F5:
        return ImGuiKey_F5;
    case Key::F6:
        return ImGuiKey_F6;
    case Key::F7:
        return ImGuiKey_F7;
    case Key::F8:
        return ImGuiKey_F8;
    case Key::F9:
        return ImGuiKey_F9;
    case Key::F10:
        return ImGuiKey_F10;
    case Key::F11:
        return ImGuiKey_F11;
    case Key::F12:
        return ImGuiKey_F12;
    case Key::KP_MULTIPLY:
        return ImGuiKey_KeypadMultiply;
    case Key::KP_DIVIDE:
        return ImGuiKey_KeypadDivide;
    case Key::KP_SUBTRACT:
        return ImGuiKey_KeypadSubtract;
    case Key::KP_PERIOD:
        return ImGuiKey_KeypadDecimal;
    case Key::KP_ADD:
        return ImGuiKey_KeypadAdd;
    case Key::KP_0:
        return ImGuiKey_Keypad0;
    case Key::KP_1:
        return ImGuiKey_Keypad1;
    case Key::KP_2:
        return ImGuiKey_Keypad2;
    case Key::KP_3:
        return ImGuiKey_Keypad3;
    case Key::KP_4:
        return ImGuiKey_Keypad4;
    case Key::KP_5:
        return ImGuiKey_Keypad5;
    case Key::KP_6:
        return ImGuiKey_Keypad6;
    case Key::KP_7:
        return ImGuiKey_Keypad7;
    case Key::KP_8:
        return ImGuiKey_Keypad8;
    case Key::KP_9:
        return ImGuiKey_Keypad9;
    case Key::MENU:
        return ImGuiKey_Menu;
    case Key::SPACE:
        return ImGuiKey_Space;
    case Key::APOSTROPHE:
        return ImGuiKey_Apostrophe;
    case Key::COMMA:
        return ImGuiKey_Comma;
    case Key::MINUS:
        return ImGuiKey_Minus;
    case Key::PERIOD:
        return ImGuiKey_Period;
    case Key::SLASH:
        return ImGuiKey_Slash;
    case Key::KEY_0:
        return ImGuiKey_0;
    case Key::KEY_1:
        return ImGuiKey_1;
    case Key::KEY_2:
        return ImGuiKey_2;
    case Key::KEY_3:
        return ImGuiKey_3;
    case Key::KEY_4:
        return ImGuiKey_4;
    case Key::KEY_5:
        return ImGuiKey_5;
    case Key::KEY_6:
        return ImGuiKey_6;
    case Key::KEY_7:
        return ImGuiKey_7;
    case Key::KEY_8:
        return ImGuiKey_8;
    case Key::KEY_9:
        return ImGuiKey_9;
    case Key::SEMICOLON:
        return ImGuiKey_Semicolon;
    case Key::EQUAL:
        return ImGuiKey_Equal;
    case Key::A:
        return ImGuiKey_A;
    case Key::B:
        return ImGuiKey_B;
    case Key::C:
        return ImGuiKey_C;
    case Key::D:
        return ImGuiKey_D;
    case Key::E:
        return ImGuiKey_E;
    case Key::F:
        return ImGuiKey_F;
    case Key::G:
        return ImGuiKey_G;
    case Key::H:
        return ImGuiKey_H;
    case Key::I:
        return ImGuiKey_I;
    case Key::J:
        return ImGuiKey_J;
    case Key::K:
        return ImGuiKey_K;
    case Key::L:
        return ImGuiKey_L;
    case Key::M:
        return ImGuiKey_M;
    case Key::N:
        return ImGuiKey_N;
    case Key::O:
        return ImGuiKey_O;
    case Key::P:
        return ImGuiKey_P;
    case Key::Q:
        return ImGuiKey_Q;
    case Key::R:
        return ImGuiKey_R;
    case Key::S:
        return ImGuiKey_S;
    case Key::T:
        return ImGuiKey_T;
    case Key::U:
        return ImGuiKey_U;
    case Key::V:
        return ImGuiKey_V;
    case Key::W:
        return ImGuiKey_W;
    case Key::X:
        return ImGuiKey_X;
    case Key::Y:
        return ImGuiKey_Y;
    case Key::Z:
        return ImGuiKey_Z;
    case Key::BRACKETLEFT:
        return ImGuiKey_LeftBracket;
    case Key::BACKSLASH:
        return ImGuiKey_Backslash;
    case Key::BRACKETRIGHT:
        return ImGuiKey_RightBracket;
    case Key::QUOTELEFT:
        return ImGuiKey_GraveAccent;
    default:
        return ImGuiKey_None;
    };
}

inline ImGuiKey ToImGuiKey(JoyButton btn)
{
    switch (btn)
    {
    case JoyButton::A:
        return ImGuiKey_GamepadFaceDown;
    case JoyButton::B:
        return ImGuiKey_GamepadFaceRight;
    case JoyButton::X:
        return ImGuiKey_GamepadFaceLeft;
    case JoyButton::Y:
        return ImGuiKey_GamepadFaceUp;
    case JoyButton::BACK:
        return ImGuiKey_GamepadBack;
    case JoyButton::START:
        return ImGuiKey_GamepadStart;
    case JoyButton::LEFT_STICK:
        return ImGuiKey_GamepadL3;
    case JoyButton::RIGHT_STICK:
        return ImGuiKey_GamepadR3;
    case JoyButton::LEFT_SHOULDER:
        return ImGuiKey_GamepadL1;
    case JoyButton::RIGHT_SHOULDER:
        return ImGuiKey_GamepadR1;
    case JoyButton::DPAD_UP:
        return ImGuiKey_GamepadDpadUp;
    case JoyButton::DPAD_DOWN:
        return ImGuiKey_GamepadDpadDown;
    case JoyButton::DPAD_LEFT:
        return ImGuiKey_GamepadDpadLeft;
    case JoyButton::DPAD_RIGHT:
        return ImGuiKey_GamepadDpadRight;
    default:
        return ImGuiKey_None;
    };
}
#endif
} // namespace ImGui::Godot

#ifndef IGN_EXPORT
// widgets
namespace ImGui {
#if defined(IGN_GDEXT)
using godot::AtlasTexture;
using godot::Ref;
using godot::StringName;
using godot::SubViewport;
using godot::Texture2D;
#endif

inline bool SubViewport(SubViewport* svp)
{
    ERR_FAIL_COND_V(!ImGui::Godot::detail::GET_IMGUIGD(), false);
    static const StringName sn("SubViewport");
    return ImGui::Godot::detail::ImGuiGD->call(sn, svp);
}

inline void Image(Texture2D* tex, const Vector2& size, const Vector2& uv0 = {0, 0}, const Vector2& uv1 = {1, 1},
                  const Color& tint_col = {1, 1, 1, 1}, const Color& border_col = {0, 0, 0, 0})
{
    ImGui::Image(ImGui::Godot::BindTexture(tex), size, uv0, uv1, tint_col, border_col);
}

inline void Image(const Ref<Texture2D>& tex, const Vector2& size, const Vector2& uv0 = {0, 0},
                  const Vector2& uv1 = {1, 1}, const Color& tint_col = {1, 1, 1, 1},
                  const Color& border_col = {0, 0, 0, 0})
{
    ImGui::Image(ImGui::Godot::BindTexture(tex.ptr()), size, uv0, uv1, tint_col, border_col);
}

inline void Image(AtlasTexture* tex, const Vector2& size, const Color& tint_col = {1, 1, 1, 1},
                  const Color& border_col = {0, 0, 0, 0})
{
    ImVec2 uv0, uv1;
    ImGui::Godot::detail::GetAtlasUVs(tex, uv0, uv1);
    ImGui::Image(ImGui::Godot::BindTexture(tex), size, uv0, uv1, tint_col, border_col);
}

inline void Image(const Ref<AtlasTexture>& tex, const Vector2& size, const Color& tint_col = {1, 1, 1, 1},
                  const Color& border_col = {0, 0, 0, 0})
{
    ImVec2 uv0, uv1;
    ImGui::Godot::detail::GetAtlasUVs(tex.ptr(), uv0, uv1);
    ImGui::Image(ImGui::Godot::BindTexture(tex.ptr()), size, uv0, uv1, tint_col, border_col);
}

inline bool ImageButton(const char* str_id, Texture2D* tex, const Vector2& size, const Vector2& uv0 = {0, 0},
                        const Vector2& uv1 = {1, 1}, const Color& bg_col = {0, 0, 0, 0},
                        const Color& tint_col = {1, 1, 1, 1})
{
    return ImGui::ImageButton(str_id, ImGui::Godot::BindTexture(tex), size, uv0, uv1, bg_col, tint_col);
}

inline bool ImageButton(const char* str_id, const Ref<Texture2D>& tex, const Vector2& size, const Vector2& uv0 = {0, 0},
                        const Vector2& uv1 = {1, 1}, const Color& bg_col = {0, 0, 0, 0},
                        const Color& tint_col = {1, 1, 1, 1})
{
    return ImGui::ImageButton(str_id, ImGui::Godot::BindTexture(tex.ptr()), size, uv0, uv1, bg_col, tint_col);
}

inline bool ImageButton(const char* str_id, AtlasTexture* tex, const Vector2& size, const Color& bg_col = {0, 0, 0, 0},
                        const Color& tint_col = {1, 1, 1, 1})
{
    ImVec2 uv0, uv1;
    ImGui::Godot::detail::GetAtlasUVs(tex, uv0, uv1);
    return ImGui::ImageButton(str_id, ImGui::Godot::BindTexture(tex), size, uv0, uv1, bg_col, tint_col);
}

inline bool ImageButton(const char* str_id, const Ref<AtlasTexture>& tex, const Vector2& size,
                        const Color& bg_col = {0, 0, 0, 0}, const Color& tint_col = {1, 1, 1, 1})
{
    ImVec2 uv0, uv1;
    ImGui::Godot::detail::GetAtlasUVs(tex.ptr(), uv0, uv1);
    return ImGui::ImageButton(str_id, ImGui::Godot::BindTexture(tex.ptr()), size, uv0, uv1, bg_col, tint_col);
}
} // namespace ImGui
#endif

#ifndef IGN_GDEXT
#ifdef _WIN32
#define IGN_MOD_EXPORT __declspec(dllexport)
#else
#define IGN_MOD_EXPORT
#endif

#define IMGUI_GODOT_MODULE_INIT()                                                                         \
    extern "C" {                                                                                          \
    void IGN_MOD_EXPORT imgui_godot_module_init(uint32_t ver, ImGuiContext* ctx, ImGuiMemAllocFunc afunc, \
                                                ImGuiMemFreeFunc ffunc)                                   \
    {                                                                                                     \
        IM_ASSERT(ver == IMGUI_VERSION_NUM);                                                              \
        ImGui::SetCurrentContext(ctx);                                                                    \
        ImGui::SetAllocatorFunctions(afunc, ffunc, nullptr);                                              \
    }                                                                                                     \
    }
#endif
