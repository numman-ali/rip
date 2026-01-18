use super::*;

#[test]
fn validate_image_request_body_schemas() {
    for (schema, model) in [
        ("CreateImageBody15Param.json", "gpt-image-1.5"),
        ("CreateImageBody1MiniParam.json", "gpt-image-1-mini"),
        ("CreateImageBody1Param.json", "gpt-image-1"),
        (
            "CreateImageBodyChatGPTImageLatestParam.json",
            "chatgpt-image-latest",
        ),
    ] {
        let create = serde_json::json!({
            "model": model,
            "prompt": "make a cat"
        });
        let errors = schema_errors(schema, create);
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    for (schema, model) in [
        ("EditImageBody15Param.json", "gpt-image-1.5"),
        ("EditImageBody1MiniParam.json", "gpt-image-1-mini"),
        ("EditImageBody1Param.json", "gpt-image-1"),
        (
            "EditImageBodyChatGPTImageLatestParam.json",
            "chatgpt-image-latest",
        ),
    ] {
        let edit = serde_json::json!({
            "model": model,
            "prompt": "add a hat",
            "image": "raw-image"
        });
        let errors = schema_errors(schema, edit);
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    let edits = serde_json::json!({
        "model": "dall-e-2",
        "prompt": "add clouds",
        "image": "raw-image"
    });
    let errors = schema_errors("EditsBodyDallE2Param.json", edits);
    assert!(errors.is_empty(), "errors: {errors:?}");

    let gen2 = serde_json::json!({
        "model": "dall-e-2",
        "prompt": "a sunset"
    });
    let errors = schema_errors("GenerationsBodyDallE2Param.json", gen2);
    assert!(errors.is_empty(), "errors: {errors:?}");

    let gen3 = serde_json::json!({
        "model": "dall-e-3",
        "prompt": "a sunrise"
    });
    let errors = schema_errors("GenerationsBodyDallE3Param.json", gen3);
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_image_enums_and_usage_schemas() {
    let errors = schema_errors("ImageBackground.json", serde_json::json!("transparent"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageDetail.json", serde_json::json!("auto"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageModeration.json", serde_json::json!("auto"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageOutputFormat.json", serde_json::json!("png"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageQuality.json", serde_json::json!("high"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageQualityDallE.json", serde_json::json!("hd"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageSize.json", serde_json::json!("1024x1024"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageSizeDallE2.json", serde_json::json!("256x256"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageSizeDallE3.json", serde_json::json!("1024x1024"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageStyleDallE.json", serde_json::json!("vivid"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageGenToolModel.json", serde_json::json!("gpt-image-1"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let input_details = serde_json::json!({
        "text_tokens": 3,
        "image_tokens": 2
    });
    let output_details = serde_json::json!({
        "image_tokens": 1,
        "text_tokens": 4
    });

    let errors = schema_errors("ImageGenInputUsageDetails.json", input_details.clone());
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("ImageGenOutputTokensDetails.json", output_details.clone());
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ImageGenUsage.json",
        serde_json::json!({
            "input_tokens": 5,
            "output_tokens": 4,
            "total_tokens": 9,
            "input_tokens_details": input_details,
            "output_tokens_details": output_details
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let usage_input_details = serde_json::json!({
        "text_tokens": 2,
        "image_tokens": 1
    });
    let usage_output_details = serde_json::json!({
        "image_tokens": 2,
        "text_tokens": 3
    });

    let errors = schema_errors(
        "ImageUsageInputTokensDetails.json",
        usage_input_details.clone(),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ImageUsageOutputTokensDetails.json",
        usage_output_details.clone(),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ImageUsage.json",
        serde_json::json!({
            "input_tokens": 3,
            "output_tokens": 4,
            "total_tokens": 7,
            "input_tokens_details": usage_input_details,
            "output_tokens_details": usage_output_details
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "Image.json",
        serde_json::json!({
            "url": "https://example.com/image.png"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ImageResource.json",
        serde_json::json!({
            "created": 1,
            "data": [
                { "url": "https://example.com/image.png" }
            ]
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_video_schemas() {
    let errors = schema_errors(
        "CreateVideoBody.json",
        serde_json::json!({ "prompt": "make a video" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "CreateVideoRemixBody.json",
        serde_json::json!({ "prompt": "remix this" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("VideoModel.json", serde_json::json!("sora-2"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("VideoSeconds.json", serde_json::json!("4"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("VideoSize.json", serde_json::json!("720x1280"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("VideoStatus.json", serde_json::json!("queued"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("VideoContentVariant.json", serde_json::json!("video"));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "DeletedVideoResource.json",
        serde_json::json!({
            "object": "video.deleted",
            "deleted": true,
            "id": "vid_1"
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "VideoListResource.json",
        serde_json::json!({
            "object": "list",
            "data": [],
            "first_id": null,
            "last_id": null,
            "has_more": false
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "VideoResource.json",
        serde_json::json!({
            "id": "vid_1",
            "object": "video",
            "model": "sora-2",
            "status": "queued",
            "progress": 0,
            "created_at": 1,
            "completed_at": null,
            "expires_at": null,
            "prompt": "a clip",
            "size": "720x1280",
            "seconds": "4",
            "remixed_from_video_id": null,
            "error": null
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn validate_empty_model_param_schema() {
    let errors = schema_errors("EmptyModelParam.json", serde_json::json!({}));
    assert!(errors.is_empty(), "errors: {errors:?}");
}
