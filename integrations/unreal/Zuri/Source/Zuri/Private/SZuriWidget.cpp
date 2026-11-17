#include "SZuriWidget.h"

#include "Engine/Texture2D.h"
#include "Framework/Application/SlateApplication.h"
#include "Input/Events.h"
#include "Input/Reply.h"
#include "InputCoreTypes.h"
#include "Layout/Geometry.h"
#include "Rendering/DrawElements.h"
#include "Styling/WidgetStyle.h"
#include "ZuriInstance.h"

void SZuriWidget::Construct(const FArguments& InArgs)
{
	bIsFocusable = InArgs._IsFocusable;
	SetCanTick(true);
}

void SZuriWidget::SetInstance(UZuriInstance* InInstance)
{
	if (Instance.Get() != InInstance)
	{
		if (Instance && Pressed.IsSet())
		{
			// The press that came through this widget cannot be let go through it any more.
			Instance->PointerExit(Pressed.GetValue());
		}
		Pressed.Reset();
		Instance.Reset(InInstance);
		// The keys that are down went down in the other instance.
		HeldKeys.Reset();
		Brush.SetResourceObject(Instance ? Instance->GetTexture() : nullptr);
		Invalidate(EInvalidateWidgetReason::Layout);
	}
}

void SZuriWidget::Tick(const FGeometry& AllottedGeometry, const double InCurrentTime, const float InDeltaTime)
{
	if (!Instance)
	{
		return;
	}

	Instance->Advance(InDeltaTime);
	const FVector2f PixelSize = FVector2f(AllottedGeometry.GetLocalSize()) * AllottedGeometry.Scale;
	if (Instance->Render(FMath::RoundToInt32(PixelSize.X), FMath::RoundToInt32(PixelSize.Y)))
	{
		// A resize replaces the texture.
		Brush.SetResourceObject(Instance->GetTexture());
		Invalidate(EInvalidateWidgetReason::Paint);
	}
}

int32 SZuriWidget::OnPaint(const FPaintArgs& Args, const FGeometry& AllottedGeometry, const FSlateRect& MyCullingRect, FSlateWindowElementList& OutDrawElements, int32 LayerId, const FWidgetStyle& InWidgetStyle, bool bParentEnabled) const
{
	// A brush without a texture would paint a white box.
	if (Brush.GetResourceObject())
	{
		const ESlateDrawEffect DrawEffects = ShouldBeEnabled(bParentEnabled) ? ESlateDrawEffect::None : ESlateDrawEffect::DisabledEffect;
		FSlateDrawElement::MakeBox(OutDrawElements, LayerId, AllottedGeometry.ToPaintGeometry(), &Brush, DrawEffects, InWidgetStyle.GetColorAndOpacityTint());
	}
	return LayerId;
}

FVector2D SZuriWidget::ComputeDesiredSize(float LayoutScaleMultiplier) const
{
	return Instance ? Instance->GetArtboardSize() : FVector2D::ZeroVector;
}

FVector2D SZuriWidget::ToRenderTarget(const FGeometry& Geometry, const FPointerEvent& Event) const
{
	// The render target covers the widget at layout scale, as sized in Tick.
	return FVector2D(Geometry.AbsoluteToLocal(Event.GetScreenSpacePosition())) * Geometry.Scale;
}

// The runtime follows one pointer and one button, the press's: it is told which pointer and which button each
// event is of, and says whether it took it.
static int32 ButtonOf(const FPointerEvent& Event)
{
	return Event.GetEffectingButton() == EKeys::LeftMouseButton ? 0 : 1;
}

void SZuriWidget::OnMouseEnter(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent)
{
	if (Instance)
	{
		Instance->PointerMove(ToRenderTarget(MyGeometry, MouseEvent), MouseEvent.GetPointerIndex());
	}
}

void SZuriWidget::OnMouseLeave(const FPointerEvent& MouseEvent)
{
	if (Instance)
	{
		Instance->PointerExit(MouseEvent.GetPointerIndex());
	}
}

FReply SZuriWidget::OnMouseMove(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent)
{
	if (Instance)
	{
		Instance->PointerMove(ToRenderTarget(MyGeometry, MouseEvent), MouseEvent.GetPointerIndex());
	}
	return FReply::Unhandled();
}

// Slate tells of every capture that ends, also the one this widget gives back at a release. One that ends while
// the press it was taken for is open was taken away (the window lost the focus, another widget took the capture):
// for the scene the pointer has left.
void SZuriWidget::OnMouseCaptureLost(const FCaptureLostEvent& CaptureLostEvent)
{
	if (Instance && Pressed == CaptureLostEvent.PointerIndex)
	{
		Instance->PointerExit(CaptureLostEvent.PointerIndex);
		Pressed.Reset();
	}
}

// Touches arrive here too: Slate replays a touch no widget handled as the left mouse button.
FReply SZuriWidget::OnMouseButtonDown(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent)
{
	if (!Instance || !Instance->PointerDown(ToRenderTarget(MyGeometry, MouseEvent), MouseEvent.GetPointerIndex(), ButtonOf(MouseEvent)))
	{
		return FReply::Unhandled();
	}
	Pressed = static_cast<int32>(MouseEvent.GetPointerIndex());
	// How far a press may go before it scrolls a Layout: as far as Slate lets one go before it is a drag. That
	// is in pixels of the screen, as the render target is.
	Instance->SetDragThreshold(FSlateApplication::Get().GetDragTriggerDistance());
	FReply Reply = FReply::Handled().CaptureMouse(AsShared());
	if (bIsFocusable)
	{
		Reply.SetUserFocus(AsShared(), EFocusCause::Mouse);
	}
	return Reply;
}

FReply SZuriWidget::OnMouseButtonUp(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent)
{
	if (!Instance || ButtonOf(MouseEvent) != 0)
	{
		return FReply::Unhandled();
	}
	Instance->PointerUp(ToRenderTarget(MyGeometry, MouseEvent), MouseEvent.GetPointerIndex());
	if (Pressed == static_cast<int32>(MouseEvent.GetPointerIndex()))
	{
		Pressed.Reset();
	}
	if (MouseEvent.IsTouchEvent())
	{
		// A lifted finger hovers over nothing.
		Instance->PointerExit(MouseEvent.GetPointerIndex());
	}
	// Whatever the runtime made of the release, this pointer's capture is given back.
	return FReply::Handled().ReleaseMouseCapture();
}

bool SZuriWidget::SupportsKeyboardFocus() const
{
	return bIsFocusable;
}

/** The runtime's name, which is the web's KeyboardEvent.key, for a key that types no character; null for the other keys. */
static const TCHAR* NamedKey(const FKey& Key)
{
	static const TMap<FKey, const TCHAR*> Names = {
		{ EKeys::Enter, TEXT("Enter") }, { EKeys::Escape, TEXT("Escape") }, { EKeys::Tab, TEXT("Tab") },
		{ EKeys::Left, TEXT("ArrowLeft") }, { EKeys::Right, TEXT("ArrowRight") }, { EKeys::Up, TEXT("ArrowUp") }, { EKeys::Down, TEXT("ArrowDown") },
		{ EKeys::BackSpace, TEXT("Backspace") }, { EKeys::Delete, TEXT("Delete") }, { EKeys::Home, TEXT("Home") }, { EKeys::End, TEXT("End") },
		{ EKeys::PageUp, TEXT("PageUp") }, { EKeys::PageDown, TEXT("PageDown") },
	};
	return Names.FindRef(Key);
}

// Each handler keeps a key only if the scene reacted to it, so that the others still navigate the UI.
// A held key is sent once: its repeats are not sent, and are the scene's if the first press was.

FReply SZuriWidget::PressKey(const FKey& Key, const FString& Name)
{
	const bool bHandled = Instance && Instance->Key(Name, true);
	HeldKeys.Add(Key, { Name, bHandled });
	return bHandled ? FReply::Handled() : FReply::Unhandled();
}

FReply SZuriWidget::RepeatKey(const FKey& Key) const
{
	const FHeldKey* Held = HeldKeys.Find(Key);
	return Held && Held->bHandled ? FReply::Handled() : FReply::Unhandled();
}

FReply SZuriWidget::OnKeyDown(const FGeometry& MyGeometry, const FKeyEvent& InKeyEvent)
{
	// A key that types a character is sent by OnKeyChar: the key event does not say which character, with Shift or on another layout.
	const TCHAR* Name = NamedKey(InKeyEvent.GetKey());
	PendingKey = Name ? EKeys::Invalid : InKeyEvent.GetKey();
	if (!Name)
	{
		return FReply::Unhandled();
	}
	return InKeyEvent.IsRepeat() ? RepeatKey(InKeyEvent.GetKey()) : PressKey(InKeyEvent.GetKey(), Name);
}

FReply SZuriWidget::OnKeyChar(const FGeometry& MyGeometry, const FCharacterEvent& InCharacterEvent)
{
	// Enter, Tab and the like arrive here too, as control characters. OnKeyDown has sent those.
	const TCHAR Character = InCharacterEvent.GetCharacter();
	if (FChar::IsControl(Character))
	{
		return FReply::Unhandled();
	}
	return InCharacterEvent.IsRepeat() ? RepeatKey(PendingKey) : PressKey(PendingKey, FString::Chr(Character));
}

FReply SZuriWidget::OnKeyUp(const FGeometry& MyGeometry, const FKeyEvent& InKeyEvent)
{
	// There is no character event for a key going up, so it is sent with the name it went down with.
	// A key with a name that went down elsewhere, before this widget had the focus, still goes up here.
	FHeldKey Held;
	if (!HeldKeys.RemoveAndCopyValue(InKeyEvent.GetKey(), Held))
	{
		if (const TCHAR* Name = NamedKey(InKeyEvent.GetKey()))
		{
			Held.Name = Name;
		}
	}
	return Instance && !Held.Name.IsEmpty() && Instance->Key(Held.Name, false) ? FReply::Handled() : FReply::Unhandled();
}

void SZuriWidget::OnFocusLost(const FFocusEvent& InFocusEvent)
{
	// The key-up of a key that is still down goes to whoever has the focus then, so the scene is told now.
	for (const TPair<FKey, FHeldKey>& Held : HeldKeys)
	{
		if (Instance)
		{
			Instance->Key(Held.Value.Name, false);
		}
	}
	HeldKeys.Reset();
}
