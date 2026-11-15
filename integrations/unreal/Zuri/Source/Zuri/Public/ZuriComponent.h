#pragma once

#include "Components/ActorComponent.h"
#include "CoreMinimal.h"
#include "UObject/ObjectPtr.h"
#include "ZuriInstance.h"

#include "ZuriComponent.generated.h"

class UMaterialInstanceDynamic;
class UZuriFile;

/**
 * Plays a Zuri artboard into a texture parameter of a dynamic material, for meshes in the world.
 * The texture is sRGB with straight alpha: use its RGB as color and its alpha as opacity.
 */
UCLASS(ClassGroup = (Zuri), meta = (BlueprintSpawnableComponent))
class ZURI_API UZuriComponent : public UActorComponent
{
	GENERATED_BODY()

public:
	UZuriComponent();

	/** The file to play. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Zuri")
	TObjectPtr<UZuriFile> File;

	/** Name of the artboard to show; empty for the file's first artboard. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Zuri")
	FString Artboard;

	/** Name of the state machine or animation to play; empty for the first state machine, or failing that the first animation. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Zuri")
	FString PlayName;

	/** How the artboard is placed in a texture of a different shape. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Zuri")
	EZuriFit Fit = EZuriFit::Contain;

	/** Where the artboard sits in the room the fit leaves over, or which part shows when it overflows. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Zuri")
	EZuriAlignment Alignment = EZuriAlignment::Center;

	/** Size of the texture in pixels. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Zuri", meta = (ClampMin = 1, ClampMax = 16384))
	FIntPoint TextureSize = FIntPoint(512, 512);

	/** The material that receives the texture, as from Create Dynamic Material Instance. */
	UPROPERTY(BlueprintReadWrite, Category = "Zuri")
	TObjectPtr<UMaterialInstanceDynamic> Material;

	/** The texture parameter of Material to set. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Zuri")
	FName TextureParameterName = TEXT("ZuriTexture");

	/**
	 * The playing instance, for its texture, view model, events and pointer input.
	 * It is created when play begins; null before that, or if File is unset or invalid.
	 */
	UFUNCTION(BlueprintPure, Category = "Zuri")
	UZuriInstance* GetInstance() const { return Instance; }

	//~ Begin UActorComponent
	virtual void BeginPlay() override;
	virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;
	//~ End UActorComponent

private:
	UPROPERTY(Transient)
	TObjectPtr<UZuriInstance> Instance;
};
