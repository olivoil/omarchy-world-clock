#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float centerLongitude;
    float centerLatitude;
    vec4 oceanColor;
    vec4 landColor;
    vec4 boundaryColor;
    vec4 rimColor;
};

layout(binding = 1) uniform sampler2D source;

const float PI = 3.14159265358979323846;

void main()
{
    vec2 disc = vec2(qt_TexCoord0.x * 2.0 - 1.0,
                     1.0 - qt_TexCoord0.y * 2.0);
    float radiusSquared = dot(disc, disc);
    if (radiusSquared > 1.0) {
        fragColor = vec4(0.0);
        return;
    }

    float depth = sqrt(max(0.0, 1.0 - radiusSquared));
    float sinLatitude = sin(centerLatitude);
    float cosLatitude = cos(centerLatitude);
    float sinLongitude = sin(centerLongitude);
    float cosLongitude = cos(centerLongitude);

    vec3 east = vec3(-sinLongitude, cosLongitude, 0.0);
    vec3 north = vec3(-sinLatitude * cosLongitude,
                      -sinLatitude * sinLongitude,
                      cosLatitude);
    vec3 center = vec3(cosLatitude * cosLongitude,
                       cosLatitude * sinLongitude,
                       sinLatitude);
    vec3 world = normalize(disc.x * east + disc.y * north + depth * center);

    float latitude = asin(clamp(world.z, -1.0, 1.0));
    float longitude = atan(world.y, world.x);
    vec2 textureCoordinate = vec2(fract(longitude / (2.0 * PI) + 0.5),
                                  clamp(0.5 - latitude / PI, 0.0, 1.0));
    // The longitude coordinate wraps from 1 back to 0 at the antimeridian.
    // Implicit derivatives see that as a full-texture jump and select a very
    // blurry mip level, which paints a vertical seam across the Pacific. Keep
    // the coordinate wrapped, but fold its gradients onto the shortest path.
    vec2 gradientX = dFdx(textureCoordinate);
    vec2 gradientY = dFdy(textureCoordinate);
    gradientX.x -= round(gradientX.x);
    gradientY.x -= round(gradientY.x);
    vec4 mapSample = textureGrad(source, textureCoordinate, gradientX, gradientY);
    float landMask = mapSample.a;
    float luminance = dot(mapSample.rgb, vec3(0.2126, 0.7152, 0.0722));
    float interior = smoothstep(0.68, 0.9, luminance);

    vec3 geography = mix(boundaryColor.rgb, landColor.rgb, interior);
    vec3 color = mix(oceanColor.rgb, geography, landMask);

    // Depth comes from the sphere itself. This quiet limb falloff is enough to
    // make rotation legible without adding a decorative light source.
    float limbLight = 0.76 + 0.24 * pow(depth, 0.42);
    color *= limbLight;
    float rim = smoothstep(0.78, 0.995, radiusSquared);
    color = mix(color, rimColor.rgb, rim * 0.18);

    float edge = 1.0 - smoothstep(0.992, 1.0, radiusSquared);
    float alpha = edge;
    fragColor = vec4(color * alpha, alpha) * qt_Opacity;
}
