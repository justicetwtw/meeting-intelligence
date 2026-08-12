FROM python:3.13-slim

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PYTHONPATH=/app/src \
    MI_LOCAL_AI_HOST=0.0.0.0 \
    MI_LOCAL_AI_PORT=8091 \
    MI_LOCAL_AI_MODE=mock

WORKDIR /app

RUN addgroup --system local-ai \
    && adduser --system --ingroup local-ai local-ai

COPY --chown=local-ai:local-ai src ./src

USER local-ai
EXPOSE 8091

CMD ["python", "-m", "meeting_intelligence_local_ai"]
