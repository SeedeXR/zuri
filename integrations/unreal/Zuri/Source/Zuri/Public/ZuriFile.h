#pragma once

#include "CoreMinimal.h"
#include "Templates/UniquePtr.h"
#include "UObject/Object.h"
#include "UObject/ObjectPtr.h"

#include "ZuriFile.generated.h"

class UAssetImportData;
struct ZuriFile;

/** Releases a runtime file handle. */
struct FZuriFileDeleter
{
	ZURI_API void operator()(ZuriFile* Handle) const;
};

/** A compiled Zuri scene (.zur): artboards with their animations, state machines and view models. */
UCLASS(BlueprintType)
class ZURI_API UZuriFile : public UObject
{
	GENERATED_BODY()

public:
	/**
	 * Replaces the contents with the bytes of a .zur file.
	 * @return false, leaving the asset as it was, if the runtime rejects the bytes.
	 */
	bool SetData(TArray<uint8> InData);

	/**
	 * The parsed file, loaded on first use and shared by every instance made from this asset.
	 * Null if the asset holds no valid data. Game thread only.
	 */
	ZuriFile* GetHandle();

	//~ Begin UObject
	virtual void BeginDestroy() override;
#if WITH_EDITORONLY_DATA
	virtual void PostInitProperties() override;
	virtual void GetAssetRegistryTags(FAssetRegistryTagsContext Context) const override;
#endif
	//~ End UObject

	/** Names of the artboards in the file, in file order. */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Zuri")
	TArray<FString> ArtboardNames;

#if WITH_EDITORONLY_DATA
	/** Where the file was imported from, for reimporting. */
	UPROPERTY(VisibleAnywhere, Instanced, Category = "Import Settings")
	TObjectPtr<UAssetImportData> AssetImportData;
#endif

private:
	/** The .zur bytes. */
	UPROPERTY()
	TArray<uint8> Data;

	TUniquePtr<ZuriFile, FZuriFileDeleter> Handle;
};
