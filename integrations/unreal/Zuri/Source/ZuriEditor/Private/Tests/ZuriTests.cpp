#include "Components/StaticMeshComponent.h"
#include "DynamicRHI.h"
#include "Engine/StaticMesh.h"
#include "Engine/StaticMeshActor.h"
#include "Engine/Texture2D.h"
#include "Engine/TextureRenderTarget2D.h"
#include "Engine/World.h"
#include "Input/Events.h"
#include "Input/HittestGrid.h"
#include "InputCoreTypes.h"
#include "Interfaces/IPluginManager.h"
#include "Kismet/KismetRenderingLibrary.h"
#include "Materials/Material.h"
#include "Materials/MaterialExpressionTextureSampleParameter2D.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Misc/AutomationTest.h"
#include "Misc/Paths.h"
#include "RHICommandList.h"
#include "RenderDeferredCleanup.h"
#include "RenderingThread.h"
#include "ShaderCompiler.h"
#include "Slate/WidgetRenderer.h"
#include "Tests/AutomationCommon.h"
#include "TextureResource.h"
#include "UObject/Package.h"
#include "Widgets/Layout/SBox.h"
#include "Widgets/SVirtualWindow.h"
#include "ZuriComponent.h"
#include "ZuriFile.h"
#include "ZuriFileFactory.h"
#include "ZuriInstance.h"
#include "ZuriTestEventSink.h"
#include "ZuriWidget.h"

#if WITH_DEV_AUTOMATION_TESTS

/** Imports a scene from the plugin's Resources folder, as dropping it on the Content Browser does; null, with the test failed, if it cannot. */
static UZuriFile* ImportScene(FAutomationTestBase& Test, const TCHAR* Filename)
{
	const FString Path = FPaths::Combine(IPluginManager::Get().FindPlugin(TEXT("Zuri"))->GetBaseDir(), TEXT("Resources"), Filename);
	bool bCanceled = false;
	UZuriFile* File = Cast<UZuriFile>(NewObject<UZuriFileFactory>()->ImportObject(UZuriFile::StaticClass(), GetTransientPackage(), NAME_None, RF_Transient, Path, nullptr, bCanceled));
	Test.TestNotNull(FString::Printf(TEXT("%s imports"), Filename), File);
	return File;
}

/** A pixel of the instance's last Render. */
static FColor PixelAt(const UZuriInstance& Instance, int32 X, int32 Y)
{
	const uint8* Pixel = &Instance.GetPixels()[(Y * Instance.GetRenderSize().X + X) * 4];
	return FColor(Pixel[0], Pixel[1], Pixel[2], Pixel[3]);
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FZuriButtonTest, "Zuri.Button", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)

/** Drives the button example the way UZuriWidget does: import, draw, settle, bind data, click. */
bool FZuriButtonTest::RunTest(const FString& Parameters)
{
	UZuriFile* File = ImportScene(*this, TEXT("button.zur"));
	if (!File)
	{
		return false;
	}
	TestTrue(TEXT("artboards"), File->ArtboardNames == TArray<FString>{ TEXT("Button") });

	AddExpectedErrorPlain(TEXT("not a .zur file"), EAutomationExpectedErrorFlags::Contains, 1);
	TestFalse(TEXT("bytes that are not a .zur are rejected"), File->SetData({ 'n', 'o', 'p', 'e' }));
	TestTrue(TEXT("a rejected load keeps the old contents"), File->ArtboardNames.Num() == 1 && File->GetHandle() != nullptr);

	AddExpectedErrorPlain(TEXT("no artboard called 'Missing'"), EAutomationExpectedErrorFlags::Contains, 1);
	TestNull(TEXT("an unknown artboard is refused"), UZuriInstance::Create(GetTransientPackage(), File, TEXT("Missing")));

	UZuriInstance* Instance = UZuriInstance::Create(GetTransientPackage(), File, TEXT("Button"));
	if (!TestNotNull(TEXT("instance"), Instance))
	{
		return false;
	}
	TestTrue(TEXT("artboard size"), Instance->GetArtboardSize() == FVector2D(320.0, 160.0));
	TestEqual(TEXT("state machines"), Instance->GetStateMachineNames().Num(), 1);
	TestEqual(TEXT("properties"), Instance->GetProperties().Num(), 5);
	TestTrue(TEXT("declared events"), Instance->GetDeclaredEvents() == TArray<FString>{ TEXT("clicked") });

	// A new instance plays its state machine: every layer is in a state and there is no timeline to seek.
	const TArray<int32> LayerStates = Instance->GetLayerStates();
	TestTrue(TEXT("every layer is in a state"), LayerStates.Num() > 0 && LayerStates[0] != INDEX_NONE);
	TestFalse(TEXT("a state machine cannot be sought"), Instance->Seek(1.0f));

	// Object ids are file indices; schema property key 3 is "x".
	uint32 Shape = 0;
	float X = 0.0f;
	while (Shape < 1000 && !Instance->GetObjectProperty(Shape, 3, X))
	{
		++Shape;
	}
	FColor NotAColor;
	TestTrue(TEXT("an object's x reads back, and is not a color"), X == 160.0f && !Instance->GetObjectPropertyColor(Shape, 3, NotAColor));

	UZuriTestEventSink* Sink = NewObject<UZuriTestEventSink>();
	Instance->OnZuriEvent.AddDynamic(Sink, &UZuriTestEventSink::HandleZuriEvent);

	// First frame.
	Instance->Advance(0.0f);
	TestTrue(TEXT("the first frame is drawn"), Instance->Render(320, 160));
	TestEqual(TEXT("button fill"), PixelAt(*Instance, 100, 80), FColor(0x4f, 0x7c, 0xff, 0xff));
	if (TestNotNull(TEXT("texture"), Instance->GetTexture()))
	{
		TestEqual(TEXT("texture width"), Instance->GetTexture()->GetSizeX(), 320);
		TestEqual(TEXT("texture height"), Instance->GetTexture()->GetSizeY(), 160);
		TestTrue(TEXT("texture format"), Instance->GetTexture()->GetPixelFormat() == PF_R8G8B8A8);
	}

	// An idle state machine settles, after which draws are skipped.
	for (int32 Frame = 0; Frame < 30; ++Frame)
	{
		Instance->Advance(1.0f / 60.0f);
		Instance->Render(320, 160);
	}
	TestFalse(TEXT("an idle instance reports no change"), Instance->Advance(1.0f / 60.0f));
	TestFalse(TEXT("an unchanged frame is not redrawn"), Instance->Render(320, 160));
	TestTrue(TEXT("a new size is redrawn"), Instance->Render(640, 320));
	TestEqual(TEXT("button fill at 2x"), PixelAt(*Instance, 200, 160), FColor(0x4f, 0x7c, 0xff, 0xff));

	// Data binding.
	const UTexture2D* Texture = Instance->GetTexture();
	TestTrue(TEXT("accent is set"), Instance->SetColor(TEXT("accent"), FColor(0xff, 0x33, 0x66)));
	TestEqual(TEXT("accent reads back"), Instance->GetColor(TEXT("accent")).ToFColorSRGB(), FColor(0xff, 0x33, 0x66));
	TestTrue(TEXT("a changed property reports a change"), Instance->Advance(0.0f));
	TestTrue(TEXT("a changed property is redrawn"), Instance->Render(640, 320));
	TestEqual(TEXT("recolored fill"), PixelAt(*Instance, 200, 160), FColor(0xff, 0x33, 0x66, 0xff));
	TestTrue(TEXT("the same size keeps its texture"), Instance->GetTexture() == Texture);
	AddExpectedErrorPlain(TEXT("no color property called 'missing'"), EAutomationExpectedErrorFlags::Contains, 1);
	TestFalse(TEXT("an unknown property is refused"), Instance->SetColor(TEXT("missing"), FColor::Black));

	// Pointer in, events out.
	Instance->PointerMove(FVector2D(320.0, 160.0));
	Instance->PointerDown(FVector2D(320.0, 160.0));
	Instance->PointerUp(FVector2D(320.0, 160.0));
	TestEqual(TEXT("clicks"), Instance->GetNumber(TEXT("clicks")), 1.0f);
	Instance->Advance(1.0f / 60.0f);
	TestTrue(TEXT("events"), Sink->Events == TArray<FName>{ FName(TEXT("clicked")) });

	// Fit and alignment: the artboard at its own size in the bottom-right corner of a surface twice as large.
	Instance->SetFit(EZuriFit::None);
	Instance->SetAlignment(EZuriAlignment::BottomRight);
	TestTrue(TEXT("a new fit and alignment are redrawn"), Instance->Render(640, 320));
	TestEqual(TEXT("nothing is drawn in the top-left quadrant"), int32(PixelAt(*Instance, 100, 80).A), 0);
	TestEqual(TEXT("the artboard sits in the bottom-right quadrant"), int32(PixelAt(*Instance, 320 + 100, 160 + 80).A), 255);

	// Reimport from the file the asset remembers.
	UZuriFileFactory* Factory = NewObject<UZuriFileFactory>();
	TArray<FString> SourceFiles;
	TestTrue(TEXT("the asset knows its source file"), Factory->CanReimport(File, SourceFiles) && SourceFiles.Num() == 1);
	TestTrue(TEXT("the asset reimports"), Factory->Reimport(File) == EReimportResult::Succeeded);
	TestTrue(TEXT("artboards after reimport"), File->ArtboardNames == TArray<FString>{ TEXT("Button") });

	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FZuriImageTest, "Zuri.Image", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)

/** An embedded PNG is decoded by Unreal and reaches the runtime with its channels in the right order. */
bool FZuriImageTest::RunTest(const FString& Parameters)
{
	UZuriFile* File = ImportScene(*this, TEXT("image.zur"));
	UZuriInstance* Instance = UZuriInstance::Create(GetTransientPackage(), File);
	if (!TestNotNull(TEXT("instance"), Instance))
	{
		return false;
	}

	Instance->Advance(0.0f);
	TestTrue(TEXT("the first frame is drawn"), Instance->Render(64, 64));
	TestEqual(TEXT("swatch color"), PixelAt(*Instance, 32, 32), FColor(200, 100, 50, 255));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FZuriEnumTest, "Zuri.Enum", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)

/** An enum property lists its options, and is set and read by an option's name or by its position. */
bool FZuriEnumTest::RunTest(const FString& Parameters)
{
	UZuriInstance* Instance = UZuriInstance::Create(GetTransientPackage(), ImportScene(*this, TEXT("enum.zur")));
	if (!TestNotNull(TEXT("instance"), Instance))
	{
		return false;
	}

	const TArray<FZuriProperty> Properties = Instance->GetProperties();
	if (TestEqual(TEXT("properties"), Properties.Num(), 2))
	{
		TestTrue(TEXT("mode is an enum"), Properties[0].Name == TEXT("mode") && Properties[0].Type == EZuriPropertyType::Enum);
		TestTrue(TEXT("with its options in order"), Properties[0].Options == TArray<FString>{ TEXT("idle"), TEXT("busy"), TEXT("done") });
		TestTrue(TEXT("a number has no options"), Properties[1].Type == EZuriPropertyType::Number && Properties[1].Options.IsEmpty());
	}

	// The file starts it on its second option.
	TestEqualSensitive(TEXT("the authored option, by name"), Instance->GetString(TEXT("mode")), FString(TEXT("busy")));
	TestEqual(TEXT("the authored option, by position"), Instance->GetNumber(TEXT("mode")), 1.0f);

	TestTrue(TEXT("set by name"), Instance->SetString(TEXT("mode"), TEXT("done")));
	TestEqual(TEXT("a name reads back as a position"), Instance->GetNumber(TEXT("mode")), 2.0f);
	TestEqualSensitive(TEXT("and as a name"), Instance->GetString(TEXT("mode")), FString(TEXT("done")));

	TestTrue(TEXT("set by position"), Instance->SetNumber(TEXT("mode"), 0.0f));
	TestEqualSensitive(TEXT("a position reads back as a name"), Instance->GetString(TEXT("mode")), FString(TEXT("idle")));
	TestEqual(TEXT("and as a position"), Instance->GetNumber(TEXT("mode")), 0.0f);

	AddExpectedErrorPlain(TEXT("with an option 'paused'"), EAutomationExpectedErrorFlags::Contains, 1);
	TestFalse(TEXT("a name that is not an option is refused"), Instance->SetString(TEXT("mode"), TEXT("paused")));
	TestEqualSensitive(TEXT("and the value stays"), Instance->GetString(TEXT("mode")), FString(TEXT("idle")));
	return true;
}

static FKeyEvent KeyEvent(const FKey& Key, bool bRepeat = false)
{
	return FKeyEvent(Key, FModifierKeysState(), 0u, bRepeat, 0u, 0u);
}

static FCharacterEvent CharacterEvent(TCHAR Character, bool bRepeat = false)
{
	return FCharacterEvent(Character, FModifierKeysState(), 0u, bRepeat);
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FZuriKeysTest, "Zuri.Keys", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)

/** Keys reach a scene's listeners: by name through the instance, and from Slate's key events through the widget. */
bool FZuriKeysTest::RunTest(const FString& Parameters)
{
	// The button: Enter clicks it.
	UZuriInstance* Button = UZuriInstance::Create(GetTransientPackage(), ImportScene(*this, TEXT("button.zur")));
	if (!TestNotNull(TEXT("button"), Button))
	{
		return false;
	}
	UZuriTestEventSink* Sink = NewObject<UZuriTestEventSink>();
	Button->OnZuriEvent.AddDynamic(Sink, &UZuriTestEventSink::HandleZuriEvent);
	for (int32 Frame = 0; Frame < 30; ++Frame)
	{
		Button->Advance(1.0f / 60.0f);
		Button->Render(320, 160);
	}
	TestFalse(TEXT("the button has settled"), Button->Advance(1.0f / 60.0f));

	TestTrue(TEXT("Enter going down is heard"), Button->Key(TEXT("Enter"), true));
	TestEqual(TEXT("clicks"), Button->GetNumber(TEXT("clicks")), 1.0f);
	TestTrue(TEXT("a key that was heard reports a change"), Button->Advance(1.0f / 60.0f));
	TestTrue(TEXT("and is redrawn"), Button->Render(320, 160));
	TestTrue(TEXT("events"), Sink->Events == TArray<FName>{ FName(TEXT("clicked")) });
	TestFalse(TEXT("a key no listener waits for is not heard"), Button->Key(TEXT("q"), true));
	TestFalse(TEXT("nor is Enter going up"), Button->Key(TEXT("Enter"), false));
	TestFalse(TEXT("nor a name in another case"), Button->Key(TEXT("enter"), true));
	TestEqual(TEXT("clicks stay"), Button->GetNumber(TEXT("clicks")), 1.0f);

	// The widget, with a scene that counts: "a" down and up by 1, space down and Escape up by 10.
	UZuriWidget* Widget = NewObject<UZuriWidget>();
	Widget->File = ImportScene(*this, TEXT("keys.zur"));
	const TSharedRef<SWidget> SlateWidget = Widget->TakeWidget();
	UZuriInstance* Instance = Widget->GetInstance();
	if (!TestNotNull(TEXT("instance"), Instance))
	{
		return false;
	}
	TestFalse(TEXT("a widget does not take the keyboard focus unless asked"), SlateWidget->SupportsKeyboardFocus());
	const FGeometry Geometry;
	const auto Counts = [Instance]() { return FIntPoint(FMath::RoundToInt32(Instance->GetNumber(TEXT("downs"))), FMath::RoundToInt32(Instance->GetNumber(TEXT("ups")))); };

	// A key that types a character is sent when the character arrives, and goes up with the same character.
	TestFalse(TEXT("a letter's key event is not the scene's"), SlateWidget->OnKeyDown(Geometry, KeyEvent(EKeys::A)).IsEventHandled());
	TestTrue(TEXT("its character is"), SlateWidget->OnKeyChar(Geometry, CharacterEvent(TEXT('a'))).IsEventHandled() && Counts() == FIntPoint(1, 0));
	TestTrue(TEXT("the key goes up as that character"), SlateWidget->OnKeyUp(Geometry, KeyEvent(EKeys::A)).IsEventHandled() && Counts() == FIntPoint(1, 1));
	TestFalse(TEXT("and only once"), SlateWidget->OnKeyUp(Geometry, KeyEvent(EKeys::A)).IsEventHandled());

	// With Shift the same key types another character, which this scene does not wait for.
	SlateWidget->OnKeyDown(Geometry, KeyEvent(EKeys::A));
	TestFalse(TEXT("a capital is another key"), SlateWidget->OnKeyChar(Geometry, CharacterEvent(TEXT('A'))).IsEventHandled());
	TestFalse(TEXT("going up too"), SlateWidget->OnKeyUp(Geometry, KeyEvent(EKeys::A)).IsEventHandled());
	TestTrue(TEXT("nothing was counted for it"), Counts() == FIntPoint(1, 1));

	SlateWidget->OnKeyDown(Geometry, KeyEvent(EKeys::SpaceBar));
	TestTrue(TEXT("space is the character it types"), SlateWidget->OnKeyChar(Geometry, CharacterEvent(TEXT(' '))).IsEventHandled() && Counts() == FIntPoint(11, 1));

	// A key with a name is sent from its key event; the control character that follows is not sent again.
	TestFalse(TEXT("Escape going down is not the scene's"), SlateWidget->OnKeyDown(Geometry, KeyEvent(EKeys::Escape)).IsEventHandled());
	TestTrue(TEXT("Escape going up is"), SlateWidget->OnKeyUp(Geometry, KeyEvent(EKeys::Escape)).IsEventHandled() && Counts() == FIntPoint(11, 11));
	TestFalse(TEXT("Enter is left to Slate"), SlateWidget->OnKeyDown(Geometry, KeyEvent(EKeys::Enter)).IsEventHandled());
	TestFalse(TEXT("and so is its control character"), SlateWidget->OnKeyChar(Geometry, CharacterEvent(TEXT('\r'))).IsEventHandled());
	TestTrue(TEXT("nothing else was counted"), Counts() == FIntPoint(11, 11));

	// A held key repeats. The scene hears the first press only, and the repeats stay the scene's.
	SlateWidget->OnKeyDown(Geometry, KeyEvent(EKeys::A));
	SlateWidget->OnKeyChar(Geometry, CharacterEvent(TEXT('a')));
	SlateWidget->OnKeyDown(Geometry, KeyEvent(EKeys::A, true));
	TestTrue(TEXT("a repeated character is the scene's and is not heard again"), SlateWidget->OnKeyChar(Geometry, CharacterEvent(TEXT('a'), true)).IsEventHandled() && Counts() == FIntPoint(12, 11));

	// Its key-up would go to whoever has the keyboard focus by then.
	SlateWidget->OnFocusLost(FFocusEvent());
	TestTrue(TEXT("a key that is down goes up when the focus is lost"), Counts() == FIntPoint(12, 12));
	TestFalse(TEXT("and not again when it is let go"), SlateWidget->OnKeyUp(Geometry, KeyEvent(EKeys::A)).IsEventHandled());
	TestFalse(TEXT("a repeat of a key that is not down here is left to Slate"), SlateWidget->OnKeyChar(Geometry, CharacterEvent(TEXT('a'), true)).IsEventHandled());
	TestTrue(TEXT("neither was counted"), Counts() == FIntPoint(12, 12));

	// The same for a key with a name: Enter, held on a button.
	UZuriWidget* ButtonWidget = NewObject<UZuriWidget>();
	ButtonWidget->File = Button->GetFile();
	const TSharedRef<SWidget> ButtonSlate = ButtonWidget->TakeWidget();
	TestTrue(TEXT("Enter is the button's"), ButtonSlate->OnKeyDown(Geometry, KeyEvent(EKeys::Enter)).IsEventHandled());
	TestTrue(TEXT("a repeated Enter is the button's too"), ButtonSlate->OnKeyDown(Geometry, KeyEvent(EKeys::Enter, true)).IsEventHandled());
	TestEqual(TEXT("and is not another click"), ButtonWidget->GetInstance()->GetNumber(TEXT("clicks")), 1.0f);
	ButtonSlate->OnKeyUp(Geometry, KeyEvent(EKeys::Enter));
	TestFalse(TEXT("a repeat after the key went up is left to Slate"), ButtonSlate->OnKeyDown(Geometry, KeyEvent(EKeys::Enter, true)).IsEventHandled());
	return true;
}

// The tests below draw. They have the NonNullRHI flag, so an editor started with -nullrhi leaves them out.

/**
 * The pixels the GPU holds for a texture or a render target, in rows from the top.
 * Empty if there is no texture or the GPU has none for it, which PixelAt reads as transparent black.
 */
static TArray<FColor> ReadGpuPixels(UTexture* Texture)
{
	TArray<FColor> Pixels;
	FTextureResource* Resource = Texture ? Texture->GetResource() : nullptr;
	if (!Resource)
	{
		return Pixels;
	}
	ENQUEUE_RENDER_COMMAND(ZuriReadGpuPixels)([Resource, &Pixels](FRHICommandListImmediate& RHICmdList)
	{
		// The render thread makes TextureRHI, so it is only known here.
		if (Resource->TextureRHI)
		{
			RHICmdList.ReadSurfaceData(Resource->TextureRHI, FIntRect(0, 0, Resource->GetSizeX(), Resource->GetSizeY()), Pixels, FReadSurfaceDataFlags());
		}
	});
	FlushRenderingCommands();
	return Pixels;
}

/** A pixel of an image that is Width wide; transparent black if the image does not have it. */
static FColor PixelAt(const TArray<FColor>& Pixels, int32 Width, int32 X, int32 Y)
{
	const int32 Index = Y * Width + X;
	return Pixels.IsValidIndex(Index) ? Pixels[Index] : FColor(0, 0, 0, 0);
}

/** The instance's last Render as colors, to compare with what the GPU holds. */
static TArray<FColor> RenderedPixels(const UZuriInstance& Instance)
{
	const TConstArrayView<uint8> Bytes = Instance.GetPixels();
	TArray<FColor> Pixels;
	for (int32 Index = 0; Index + 3 < Bytes.Num(); Index += 4)
	{
		Pixels.Add(FColor(Bytes[Index], Bytes[Index + 1], Bytes[Index + 2], Bytes[Index + 3]));
	}
	return Pixels;
}

/**
 * Tests that a color which went through a shader has the expected red, green and blue. Slate and materials decode
 * sRGB exactly and encode it with a power curve or the other way round, which moves a channel by a few steps.
 */
static void TestNearColor(FAutomationTestBase& Test, const TCHAR* What, FColor Actual, FColor Expected)
{
	const int32 Difference = FMath::Max3(FMath::Abs(Actual.R - Expected.R), FMath::Abs(Actual.G - Expected.G), FMath::Abs(Actual.B - Expected.B));
	Test.TestTrue(FString::Printf(TEXT("%s: %s is close to %s"), What, *Actual.ToString(), *Expected.ToString()), Difference <= 4);
}

static const FColor ButtonBlue(0x4f, 0x7c, 0xff);
static const FColor ButtonPink(0xff, 0x33, 0x66);

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FZuriTextureTest, "Zuri.Texture", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter | EAutomationTestFlags::NonNullRHI)

/** The GPU texture holds what Render drew, both when the texture is new and when it is updated in place. */
bool FZuriTextureTest::RunTest(const FString& Parameters)
{
	AddInfo(FString::Printf(TEXT("Zuri GPU tests run on the %s RHI"), GDynamicRHI->GetName()));

	UZuriInstance* Instance = UZuriInstance::Create(GetTransientPackage(), ImportScene(*this, TEXT("button.zur")));
	if (!TestNotNull(TEXT("instance"), Instance))
	{
		return false;
	}

	// The artboard fills the top half of a square, so that a texture which is upside down shows.
	Instance->SetAlignment(EZuriAlignment::TopCenter);
	Instance->Advance(0.0f);
	TestTrue(TEXT("the first frame is drawn"), Instance->Render(320, 320));
	UTexture2D* Texture = Instance->GetTexture();
	TArray<FColor> Gpu = ReadGpuPixels(Texture);
	// Not empty: a failed Render and a failed read are both empty, and would compare equal.
	TestTrue(TEXT("a new texture holds the frame"), Gpu.Num() == 320 * 320 && Gpu == RenderedPixels(*Instance));
	TestEqual(TEXT("button fill, red and blue in place"), PixelAt(Gpu, 320, 100, 80), ButtonBlue);
	TestEqual(TEXT("nothing in the bottom half"), int32(PixelAt(Gpu, 320, 100, 240).A), 0);

	Instance->SetColor(TEXT("accent"), ButtonPink);
	Instance->Advance(0.0f);
	TestTrue(TEXT("the same size is redrawn into the same texture"), Instance->Render(320, 320) && Instance->GetTexture() == Texture);
	Gpu = ReadGpuPixels(Texture);
	TestTrue(TEXT("an updated texture holds the frame"), Gpu.Num() == 320 * 320 && Gpu == RenderedPixels(*Instance));
	TestEqual(TEXT("recolored fill"), PixelAt(Gpu, 320, 100, 80), ButtonPink);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FZuriWidgetTest, "Zuri.Widget", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter | EAutomationTestFlags::NonNullRHI)

/** UZuriWidget drawn by Slate at a layout scale of 2, away from the window's corner, then hovered and clicked through its pointer handlers. */
bool FZuriWidgetTest::RunTest(const FString& Parameters)
{
	UZuriWidget* Widget = NewObject<UZuriWidget>();
	Widget->File = ImportScene(*this, TEXT("button.zur"));
	// The widget is taller than the artboard, which sits at its top, so that a picture which is upside down shows.
	Widget->Alignment = EZuriAlignment::TopCenter;
	Widget->IsFocusable = true;
	UZuriTestEventSink* Sink = NewObject<UZuriTestEventSink>();
	Widget->OnZuriEvent.AddDynamic(Sink, &UZuriTestEventSink::HandleZuriEvent);
	const TSharedRef<SWidget> SlateWidget = Widget->TakeWidget();
	UZuriInstance* Instance = Widget->GetInstance();
	if (!TestNotNull(TEXT("instance"), Instance))
	{
		return false;
	}

	// A window of 640 x 480 pixels at scale 2 is 320 x 240 Slate units. The widget is its lower 320 x 220 units:
	// 640 x 440 pixels, starting 40 pixels down. The artboard covers the widget's upper 640 x 320 pixels.
	const FVector2D WindowSize(640.0, 480.0);
	const TSharedRef<SVirtualWindow> Window = SNew(SVirtualWindow).Size(WindowSize);
	Window->SetContent(SNew(SBox).Padding(FMargin(0.0f, 20.0f, 0.0f, 0.0f))[SlateWidget]);
	Window->Resize(WindowSize);
	// As Slate draws a window: its shader encodes the colors for display, and the target keeps them as written.
	FWidgetRenderer* Renderer = new FWidgetRenderer(/*bUseGammaCorrection*/ true);
	UTextureRenderTarget2D* Target = NewObject<UTextureRenderTarget2D>();
	Target->ClearColor = FLinearColor::Transparent;
	Target->InitCustomFormat(640, 480, PF_B8G8R8A8, /*bForceLinearGamma*/ true);
	Target->UpdateResourceImmediate(true);
	FHittestGrid Grid;
	// Painting ticks the widget, which advances and redraws the instance.
	const auto Draw = [&](float Scale = 2.0f)
	{
		Renderer->DrawWindow(Target, Grid, Window, Scale, WindowSize, 1.0f / 60.0f);
		return ReadGpuPixels(Target);
	};

	TArray<FColor> Pixels = Draw();
	TestTrue(TEXT("the instance is drawn at the widget's size in pixels"), Instance->GetRenderSize() == FIntPoint(640, 440));
	TestNearColor(*this, TEXT("button fill"), PixelAt(Pixels, 640, 200, 200), ButtonBlue);
	TestEqual(TEXT("nothing above the widget"), int32(PixelAt(Pixels, 640, 200, 20).A), 0);
	TestEqual(TEXT("the artboard starts at the widget's top"), int32(PixelAt(Pixels, 640, 20, 60).A), 255);
	TestEqual(TEXT("nothing below the artboard"), int32(PixelAt(Pixels, 640, 200, 460).A), 0);

	Instance->SetColor(TEXT("accent"), ButtonPink);
	TestNearColor(*this, TEXT("recolored fill"), PixelAt(Draw(), 640, 200, 200), ButtonPink);

	// Window pixel (200, 240) is pixel (200, 200) of the instance, inside the button. A handler that left out the
	// layout scale or the widget's offset would send a point outside the button.
	const FVector2f Inside(200.0f, 240.0f);
	const TArray<FWidgetAndPointer> Hit = Grid.GetBubblePath(Inside, 0.0f, false);
	if (!TestTrue(TEXT("the widget is hit-testable"), Hit.Num() > 0 && Hit.Last().Widget == SlateWidget))
	{
		BeginCleanup(Renderer);
		return false;
	}
	const FGeometry Geometry = Hit.Last().Geometry;
	const auto Mouse = [](FVector2f Position)
	{
		return FPointerEvent(0, Position, Position, TSet<FKey>{ EKeys::LeftMouseButton }, EKeys::LeftMouseButton, 0.0f, FModifierKeysState());
	};

	SlateWidget->OnMouseEnter(Geometry, Mouse(Inside));
	TestTrue(TEXT("the pointer hovers the button"), Instance->GetBool(TEXT("hover")));
	const FReply Down = SlateWidget->OnMouseButtonDown(Geometry, Mouse(Inside));
	TestTrue(TEXT("the left button presses it"), Down.IsEventHandled() && Instance->GetBool(TEXT("pressed")));
	TestTrue(TEXT("and gives a focusable widget the keyboard focus"), SlateWidget->SupportsKeyboardFocus() && Down.GetUserFocusRecepient() == SlateWidget);
	TestTrue(TEXT("and releases it"), SlateWidget->OnMouseButtonUp(Geometry, Mouse(Inside)).IsEventHandled() && !Instance->GetBool(TEXT("pressed")));
	TestEqual(TEXT("clicks"), Instance->GetNumber(TEXT("clicks")), 1.0f);
	Draw();
	TestTrue(TEXT("the click's event reaches the widget's delegate"), Sink->Events == TArray<FName>{ FName(TEXT("clicked")) });
	SlateWidget->OnMouseMove(Geometry, Mouse(FVector2f(20.0f, 60.0f)));
	TestFalse(TEXT("the pointer leaves the button"), Instance->GetBool(TEXT("hover")));

	// A touch, as Slate hands it to the mouse handlers.
	const FPointerEvent Touch(0u, 0u, Inside, Inside, 1.0f, true);
	SlateWidget->OnMouseButtonDown(Geometry, Touch);
	TestTrue(TEXT("a touch presses the button"), Instance->GetBool(TEXT("pressed")) && Instance->GetBool(TEXT("hover")));
	SlateWidget->OnMouseButtonUp(Geometry, Touch);
	TestEqual(TEXT("clicks after the touch"), Instance->GetNumber(TEXT("clicks")), 2.0f);
	TestFalse(TEXT("a lifted finger hovers over nothing"), Instance->GetBool(TEXT("hover")));

	// Keys, which Slate gives to the widget that has the keyboard focus: Enter clicks the button.
	TestTrue(TEXT("Enter is the scene's"), SlateWidget->OnKeyDown(Geometry, KeyEvent(EKeys::Enter)).IsEventHandled());
	TestEqual(TEXT("clicks after Enter"), Instance->GetNumber(TEXT("clicks")), 3.0f);
	TestFalse(TEXT("a key the scene does not wait for is left to Slate"), SlateWidget->OnKeyDown(Geometry, KeyEvent(EKeys::Tab)).IsEventHandled());
	TestEqual(TEXT("clicks after Tab"), Instance->GetNumber(TEXT("clicks")), 3.0f);

	// At scale 1 the widget is 640 x 460 pixels, starting 20 pixels down: a new size, so a new texture for the brush.
	Pixels = Draw(1.0f);
	TestTrue(TEXT("a new layout scale is a new size in pixels"), Instance->GetRenderSize() == FIntPoint(640, 460));
	TestNearColor(*this, TEXT("button fill in the new texture"), PixelAt(Pixels, 640, 200, 180), ButtonPink);

	BeginCleanup(Renderer);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FZuriComponentTest, "Zuri.Component", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter | EAutomationTestFlags::NonNullRHI)

/** UZuriComponent beside a mesh: the mesh's dynamic material gets the instance's texture and shows its pixels. */
bool FZuriComponentTest::RunTest(const FString& Parameters)
{
	FTestWorldWrapper WorldWrapper;
	if (!WorldWrapper.CreateTestWorld(EWorldType::Game) || !WorldWrapper.BeginPlayInTestWorld())
	{
		WorldWrapper.ForwardErrorMessages(this);
		return false;
	}
	UWorld* World = WorldWrapper.GetTestWorld();

	// An unlit material whose color is the texture parameter.
	UMaterial* Material = NewObject<UMaterial>();
	Material->SetShadingModel(MSM_Unlit);
	UMaterialExpressionTextureSampleParameter2D* Sample = NewObject<UMaterialExpressionTextureSampleParameter2D>(Material);
	Sample->ParameterName = TEXT("ZuriTexture");
	Material->GetExpressionCollection().AddExpression(Sample);
	Material->GetEditorOnlyData()->EmissiveColor.Connect(0, Sample);
	Material->PostEditChange();
	GShaderCompilingManager->FinishAllCompilation();

	AStaticMeshActor* Actor = World->SpawnActor<AStaticMeshActor>();
	UStaticMeshComponent* Mesh = Actor->GetStaticMeshComponent();
	Mesh->SetMobility(EComponentMobility::Movable);
	Mesh->SetStaticMesh(LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Plane.Plane")));
	UMaterialInstanceDynamic* Dynamic = Mesh->CreateDynamicMaterialInstance(0, Material);

	UZuriComponent* Component = NewObject<UZuriComponent>(Actor);
	Component->File = ImportScene(*this, TEXT("button.zur"));
	Component->TextureSize = FIntPoint(320, 160);
	Component->Material = Dynamic;
	// The actor is playing, so registering the component begins its play.
	Component->RegisterComponent();
	UZuriInstance* Instance = Component->GetInstance();
	if (!TestNotNull(TEXT("play begins with an instance"), Instance))
	{
		return false;
	}

	UTextureRenderTarget2D* Target = UKismetRenderingLibrary::CreateRenderTarget2D(World, 320, 160, RTF_RGBA8_SRGB);
	const auto Draw = [&]()
	{
		UKismetRenderingLibrary::DrawMaterialToRenderTarget(World, Target, Dynamic);
		return ReadGpuPixels(Target);
	};
	const auto BoundTexture = [&]()
	{
		UTexture* Bound = nullptr;
		Dynamic->GetTextureParameterValue(FName(TEXT("ZuriTexture")), Bound);
		return Bound;
	};

	WorldWrapper.TickTestWorld();
	TestTrue(TEXT("a tick gives the material the instance's texture"), Instance->GetTexture() && BoundTexture() == Instance->GetTexture());
	TestNearColor(*this, TEXT("the material shows the button"), PixelAt(Draw(), 320, 100, 80), ButtonBlue);

	Instance->SetColor(TEXT("accent"), ButtonPink);
	WorldWrapper.TickTestWorld();
	TestNearColor(*this, TEXT("the material shows a redrawn frame"), PixelAt(Draw(), 320, 100, 80), ButtonPink);

	UTexture* Small = BoundTexture();
	Component->TextureSize = FIntPoint(640, 320);
	WorldWrapper.TickTestWorld();
	TestTrue(TEXT("a new size gives the material the new texture"), BoundTexture() != Small && BoundTexture() == Instance->GetTexture() && Instance->GetRenderSize() == FIntPoint(640, 320));
	TestNearColor(*this, TEXT("the material shows the new texture"), PixelAt(Draw(), 320, 100, 80), ButtonPink);
	return true;
}

#endif
