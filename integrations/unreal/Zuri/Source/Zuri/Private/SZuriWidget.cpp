#include "SZuriWidget.h"

#include "Engine/Texture2D.h"
#include "Input/Events.h"
#include "Input/Reply.h"
#include "InputCoreTypes.h"
#include "Layout/Geometry.h"
#include "Rendering/DrawElements.h"
#include "Styling/WidgetStyle.h"
#include "ZuriInstance.h"

void SZuriWidget::Construct(const FArguments& InArgs)
{
	SetCanTick(true);
}

void SZuriWidget::SetInstance(UZuriInstance* InInstance)
{
	if (Instance.Get() != InInstance)
	{
		Instance.Reset(InInstance);
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

void SZuriWidget::OnMouseEnter(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent)
{
	if (Instance)
	{
		Instance->PointerMove(ToRenderTarget(MyGeometry, MouseEvent));
	}
}

void SZuriWidget::OnMouseLeave(const FPointerEvent& MouseEvent)
{
	if (Instance)
	{
		Instance->PointerExit();
	}
}

FReply SZuriWidget::OnMouseMove(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent)
{
	if (Instance)
	{
		Instance->PointerMove(ToRenderTarget(MyGeometry, MouseEvent));
	}
	return FReply::Unhandled();
}

// Touches arrive here too: Slate replays a touch no widget handled as the left mouse button.
FReply SZuriWidget::OnMouseButtonDown(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent)
{
	if (!Instance || MouseEvent.GetEffectingButton() != EKeys::LeftMouseButton)
	{
		return FReply::Unhandled();
	}
	Instance->PointerDown(ToRenderTarget(MyGeometry, MouseEvent));
	return FReply::Handled().CaptureMouse(AsShared());
}

FReply SZuriWidget::OnMouseButtonUp(const FGeometry& MyGeometry, const FPointerEvent& MouseEvent)
{
	if (!Instance || MouseEvent.GetEffectingButton() != EKeys::LeftMouseButton)
	{
		return FReply::Unhandled();
	}
	Instance->PointerUp(ToRenderTarget(MyGeometry, MouseEvent));
	if (MouseEvent.IsTouchEvent())
	{
		// A lifted finger hovers over nothing.
		Instance->PointerExit();
	}
	return FReply::Handled().ReleaseMouseCapture();
}
