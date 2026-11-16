#include "ZuriFileFactory.h"

#include "EditorFramework/AssetImportData.h"
#include "Factories/SoundFactory.h"
#include "Misc/FeedbackContext.h"
#include "Misc/FileHelper.h"
#include "Sound/SoundWave.h"
#include "ZuriFile.h"

/**
 * Makes a Sound Wave of each sound in the file, with the engine's own importer, and keeps it inside the asset.
 * A sound the importer cannot read is reported by it and left out: its event still fires, silently.
 */
static void ImportSounds(UZuriFile* File, FFeedbackContext* Warn)
{
	USoundFactory* Factory = NewObject<USoundFactory>();
	Factory->SuppressImportDialogs();
	for (const TPair<uint32, TArray<uint8>>& Encoded : File->GetEncodedSounds())
	{
		const uint8* Bytes = Encoded.Value.GetData();
		// The engine reads WAV itself. It converts anything else first, where the editor has a converter (see USoundFactory).
		const TCHAR* Type = Encoded.Value.Num() >= 4 && FMemory::Memcmp(Bytes, "RIFF", 4) == 0 ? TEXT("WAV") : TEXT("");
		// A new name each time: on a reimport the old wave may be playing, and is collected once nothing refers to it.
		const FName Name = MakeUniqueObjectName(File, USoundWave::StaticClass(), TEXT("Sound"));
		if (USoundWave* Wave = Cast<USoundWave>(Factory->FactoryCreateBinary(USoundWave::StaticClass(), File, Name, File->GetMaskedFlags(RF_PropagateToSubObjects), nullptr, Type, Bytes, Bytes + Encoded.Value.Num(), Warn)))
		{
			File->Sounds.Add(Encoded.Key, Wave);
		}
	}
}

UZuriFileFactory::UZuriFileFactory()
{
	SupportedClass = UZuriFile::StaticClass();
	Formats.Add(TEXT("zur;Zuri file"));
	bEditorImport = true;
}

UObject* UZuriFileFactory::FactoryCreateBinary(UClass* InClass, UObject* InParent, FName InName, EObjectFlags Flags, UObject* Context, const TCHAR* Type, const uint8*& Buffer, const uint8* BufferEnd, FFeedbackContext* Warn)
{
	UZuriFile* File = NewObject<UZuriFile>(InParent, InClass, InName, Flags);
	if (!File->SetData(TArray<uint8>(Buffer, static_cast<int32>(BufferEnd - Buffer))))
	{
		Warn->Logf(ELogVerbosity::Error, TEXT("%s is not a valid Zuri file; LogZuri has the reason."), *GetCurrentFilename());
		File->MarkAsGarbage();
		return nullptr;
	}
	ImportSounds(File, Warn);
	File->AssetImportData->Update(GetCurrentFilename());
	return File;
}

bool UZuriFileFactory::CanReimport(UObject* Obj, TArray<FString>& OutFilenames)
{
	const UZuriFile* File = Cast<UZuriFile>(Obj);
	if (!File || !File->AssetImportData)
	{
		return false;
	}
	File->AssetImportData->ExtractFilenames(OutFilenames);
	return true;
}

void UZuriFileFactory::SetReimportPaths(UObject* Obj, const TArray<FString>& NewReimportPaths)
{
	UZuriFile* File = Cast<UZuriFile>(Obj);
	if (File && File->AssetImportData && NewReimportPaths.Num() == 1)
	{
		File->AssetImportData->UpdateFilenameOnly(NewReimportPaths[0]);
	}
}

EReimportResult::Type UZuriFileFactory::Reimport(UObject* Obj)
{
	UZuriFile* File = Cast<UZuriFile>(Obj);
	if (!File || !File->AssetImportData)
	{
		return EReimportResult::Failed;
	}

	const FString Filename = File->AssetImportData->GetFirstFilename();
	TArray<uint8> Bytes;
	if (!FFileHelper::LoadFileToArray(Bytes, *Filename) || !File->SetData(MoveTemp(Bytes)))
	{
		return EReimportResult::Failed;
	}
	ImportSounds(File, GWarn);
	File->AssetImportData->Update(Filename);
	File->MarkPackageDirty();
	return EReimportResult::Succeeded;
}
