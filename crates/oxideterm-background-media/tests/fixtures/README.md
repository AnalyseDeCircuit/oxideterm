The H.264 fixture contains one second of red followed by one second of blue,
at 25 frames per second, 64×48 pixels, without audio. It was generated independently
of the decoder with FFmpeg/libx264 (baseline profile, 8-bit YUV420):

```sh
ffmpeg -f lavfi -i 'color=c=red:s=64x48:r=25:d=1' \
  -f lavfi -i 'color=c=blue:s=64x48:r=25:d=1' \
  -filter_complex '[0:v][1:v]concat=n=2:v=1:a=0[out]' -map '[out]' \
  -c:v libx264 -pix_fmt yuv420p -profile:v baseline -movflags +faststart red-blue.mp4
```

FFmpeg generates test inputs only; the application uses native video decoders.
