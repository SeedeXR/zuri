#include "Engine/Texture2D.h"
#include "Interfaces/IPluginManager.h"
#include "Misc/AutomationTest.h"
#include "Misc/Paths.h"
#include "UObject/Package.h"
#include "ZuriFile.h"
#include "ZuriFileFactory.h"
#include "ZuriInstance.h"
#include "ZuriTestEventSink.h"

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

#endif
