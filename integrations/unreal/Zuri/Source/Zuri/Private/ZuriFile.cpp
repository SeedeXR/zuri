#include "ZuriFile.h"

#include "ImageCore.h"
#include "ImageUtils.h"
#include "Sound/SoundWave.h"
#include "ZuriRuntime.h"

#if WITH_EDITORONLY_DATA
#include "EditorFramework/AssetImportData.h"
#include "UObject/AssetRegistryTagsContext.h"
#endif

void FZuriFileDeleter::operator()(ZuriFile* Handle) const
{
	zuri_file_free(Handle);
}

/** Parses .zur bytes and gives the runtime the pixels of every embedded image, which the host decodes. */
static TUniquePtr<ZuriFile, FZuriFileDeleter> LoadFile(const TArray<uint8>& Data, const FString& Name)
{
	TUniquePtr<ZuriFile, FZuriFileDeleter> File(zuri_file_load(Data.GetData(), static_cast<size_t>(Data.Num())));
	if (!File)
	{
		UE_LOG(LogZuri, Error, TEXT("%s: %s"), *Name, *Zuri::LastError());
		return File;
	}

	const uint32 ImageCount = zuri_file_image_count(File.Get());
	for (uint32 Index = 0; Index < ImageCount; ++Index)
	{
		const uint32 Id = zuri_file_image_id(File.Get(), Index);
		size_t EncodedSize = 0;
		const uint8* Encoded = zuri_file_image_bytes(File.Get(), Id, &EncodedSize);

		FImage Image;
		if (!FImageUtils::DecompressImage(Encoded, static_cast<int64>(EncodedSize), Image))
		{
			UE_LOG(LogZuri, Warning, TEXT("%s: image %u is in a format Unreal cannot decode (WebP, for one) and will not be drawn."), *Name, Id);
			continue;
		}

		// The runtime takes straight-alpha RGBA8; FColor is BGRA in memory.
		Image.ChangeFormat(ERawImageFormat::BGRA8, EGammaSpace::sRGB);
		for (FColor& Pixel : Image.AsBGRA8())
		{
			Swap(Pixel.B, Pixel.R);
		}
		if (!zuri_file_set_image(File.Get(), Id, static_cast<uint32>(Image.SizeX), static_cast<uint32>(Image.SizeY), Image.RawData.GetData()))
		{
			UE_LOG(LogZuri, Warning, TEXT("%s: image %u: %s"), *Name, Id, *Zuri::LastError());
		}
	}
	return File;
}

bool UZuriFile::SetData(TArray<uint8> InData)
{
	TUniquePtr<ZuriFile, FZuriFileDeleter> NewHandle = LoadFile(InData, GetName());
	if (!NewHandle)
	{
		return false;
	}

	Data = MoveTemp(InData);
	Handle = MoveTemp(NewHandle);
	ArtboardNames = Zuri::ReadNames(Handle.Get(), zuri_file_artboard_count(Handle.Get()), zuri_file_artboard_name);
	// They were the old file's.
	Sounds.Reset();
	return true;
}

TMap<uint32, TArray<uint8>> UZuriFile::GetEncodedSounds()
{
	TMap<uint32, TArray<uint8>> Encoded;
	const ZuriFile* Loaded = GetHandle();
	for (uint32 Index = 0, Count = Loaded ? zuri_file_sound_count(Loaded) : 0; Index < Count; ++Index)
	{
		const uint32 Id = zuri_file_sound_id(Loaded, Index);
		size_t Length = 0;
		const uint8* Bytes = zuri_file_sound_bytes(Loaded, Id, &Length);
		Encoded.Add(Id, TArray<uint8>(Bytes, static_cast<int32>(Length)));
	}
	return Encoded;
}

ZuriFile* UZuriFile::GetHandle()
{
	if (!Handle && !Data.IsEmpty())
	{
		Handle = LoadFile(Data, GetName());
	}
	return Handle.Get();
}

void UZuriFile::BeginDestroy()
{
	// Here rather than in the destructor, which may run off the game thread.
	Handle.Reset();
	Super::BeginDestroy();
}

#if WITH_EDITORONLY_DATA
void UZuriFile::PostInitProperties()
{
	if (!HasAnyFlags(RF_ClassDefaultObject))
	{
		AssetImportData = NewObject<UAssetImportData>(this, TEXT("AssetImportData"));
	}
	Super::PostInitProperties();
}

void UZuriFile::GetAssetRegistryTags(FAssetRegistryTagsContext Context) const
{
	if (AssetImportData)
	{
		Context.AddTag(FAssetRegistryTag(SourceFileTagName(), AssetImportData->GetSourceData().ToJson(), FAssetRegistryTag::TT_Hidden));
	}
	Super::GetAssetRegistryTags(Context);
}
#endif
